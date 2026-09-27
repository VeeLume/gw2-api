use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{DeriveInput, Expr, FnArg, Ident, ItemFn, LitStr, Token, parse_macro_input};

// ── Path helpers ─────────────────────────────────────────────────────────────

/// `"account"` → `"Account"`, `"item_price"` → `"ItemPrice"`
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|seg| {
            let mut c = seg.chars();
            match c.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}

/// Derive the endpoint struct name and accessor method from a path string.
///
/// `"items"` → endpoint=`ItemsEndpoint`, accessor=`items`, parent=None
/// `"account/wallet"` → endpoint=`WalletEndpoint`, accessor=`wallet`, parent=`AccountEndpoint`
fn path_to_endpoint(path: &str, override_accessor: Option<Ident>) -> (Ident, Ident, Option<Ident>) {
    let segments: Vec<&str> = path.split('/').collect();
    let last = *segments.last().unwrap_or(&"unknown");
    let endpoint_name = format_ident!("{}Endpoint", to_pascal_case(last));
    let accessor_name = override_accessor.unwrap_or_else(|| format_ident!("{}", last));
    let parent = if segments.len() >= 2 {
        let parent_seg = segments[segments.len() - 2];
        Some(format_ident!("{}Endpoint", to_pascal_case(parent_seg)))
    } else {
        None
    };
    (endpoint_name, accessor_name, parent)
}

/// For a function annotation: if fn_name != last path segment, treat fn_name
/// as an extra segment. Returns (parent_endpoint, method_name).
///
/// `path="recipes/search", fn_name="input"` → parent=`SearchEndpoint`, method=`input`
/// `path="commerce/exchange/coins", fn_name="coins"` → parent=`ExchangeEndpoint`, method=`coins`
/// `path="createsubtoken", fn_name="createsubtoken"` → parent=None (on Gw2Client)
fn fn_parent_from_path(path: &str, fn_name: &Ident) -> (Option<Ident>, Ident) {
    let mut segments: Vec<&str> = path.split('/').collect();
    let last = *segments.last().unwrap_or(&"");
    let fn_str = fn_name.to_string();

    // If fn name differs from last segment, append fn name as an extra segment.
    if fn_str != last {
        segments.push(&fn_str);
    }

    let method_name = format_ident!("{}", *segments.last().unwrap());
    let parent = if segments.len() >= 2 {
        let parent_seg = segments[segments.len() - 2];
        Some(format_ident!("{}Endpoint", to_pascal_case(parent_seg)))
    } else {
        None
    };
    (parent, method_name)
}

/// Build the accessor chain string shown in the CLI registry.
///
/// All segments of `path` become `.seg()` calls (no leading `client.`).
/// `terminal` is the final method appended with its args, e.g. `("get", "id")`.
/// Pass `terminal = None` for namespace handles that need no terminal call.
///
/// `path="items",           terminal=Some(("get","id"))` → `".items().get(id)"`
/// `path="account",         terminal=Some(("get",""))`   → `".account().get()"`
/// `path="commerce/exchange",terminal=None`              → `".commerce().exchange()"`
/// `path="recipes/search",  terminal=Some(("input","input"))` → `".recipes().search().input(input)"`
fn accessor_chain(path: &str, terminal: Option<(&str, &str)>) -> String {
    let mut chain = String::new();
    for seg in path.split('/') {
        chain.push('.');
        chain.push_str(seg);
        chain.push_str("()");
    }
    if let Some((method, args)) = terminal {
        chain.push('.');
        chain.push_str(method);
        chain.push('(');
        chain.push_str(args);
        chain.push(')');
    }
    chain
}

// ── Argument parsing ──────────────────────────────────────────────────────────

/// How id_type is specified: either a primitive that triggers newtype generation,
/// or an existing type ident that is used as-is.
#[derive(Clone)]
enum IdType {
    /// Generate `FooId(u32)`, `FooId(u64)`, or `FooId(String)` newtype.
    Generate(Ident), // the primitive ident: u32 / u64 / String
    /// Use an existing type directly (e.g. `ItemId`).
    Existing(Ident),
}

impl IdType {
    fn is_primitive(s: &str) -> bool {
        matches!(s, "u32" | "u64" | "String")
    }

    fn as_ident(&self, struct_name: &Ident) -> Ident {
        match self {
            IdType::Generate(_) => format_ident!("{}Id", struct_name),
            IdType::Existing(t) => t.clone(),
        }
    }
}

/// Arguments for `#[gw2_endpoint(...)]` on a struct.
struct EndpointStructArgs {
    /// Primary path. When `extra_paths` is non-empty, this is the first path in a multi-path group.
    path: LitStr,
    /// Additional paths that share the same struct type (e.g. 4 transaction endpoints).
    /// Each generates an independent endpoint handle + registry entry. No trait impl is
    /// emitted for extra paths — the single `CollectionSingletonResource` impl uses `path`.
    extra_paths: Vec<LitStr>,
    /// None → Pattern 2 (no ID). Some → Pattern 1 (ID resource).
    id_type: Option<IdType>,
    /// Pattern 1: enable pagination (.all(), .pages()).
    paged: bool,
    /// Pattern 1: suppress auto-generated `impl Patchable`.
    no_default_patch: bool,
    /// Pattern 2: .get() returns Vec<Self> instead of Self.
    collection: bool,
    /// Pattern 2+collection: .get() returns Vec<Option<Self>>.
    nullable: bool,
    /// Pattern 2: navigation-only handle — no .get().
    namespace: bool,
    /// Suppress registry entry even for namespace endpoints.
    /// Use for paths that return an error object rather than Vec<String>.
    no_registry: bool,
    auth: bool,
    accessor: Option<Ident>,
    no_accessor: bool,
    no_trait: bool,
}

impl Parse for EndpointStructArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut path = None;
        let mut id_type = None;
        let mut paged = false;
        let mut no_default_patch = false;
        let mut collection = false;
        let mut nullable = false;
        let mut namespace = false;
        let mut no_registry = false;
        let mut auth = false;
        let mut accessor = None;
        let mut no_accessor = false;
        let mut no_trait = false;
        let mut extra_paths: Vec<LitStr> = Vec::new();

        loop {
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "path" => {
                    input.parse::<Token![=]>()?;
                    path = Some(input.parse::<LitStr>()?);
                }
                "id_type" => {
                    input.parse::<Token![=]>()?;
                    let ident: Ident = input.parse()?;
                    id_type = Some(if IdType::is_primitive(&ident.to_string()) {
                        IdType::Generate(ident)
                    } else {
                        IdType::Existing(ident)
                    });
                }
                "paged" => {
                    paged = true;
                }
                "no_default_patch" => {
                    no_default_patch = true;
                }
                "collection" => {
                    collection = true;
                }
                "nullable" => {
                    nullable = true;
                }
                "namespace" => {
                    namespace = true;
                }
                "no_registry" => {
                    no_registry = true;
                }
                "auth" => {
                    auth = true;
                }
                "accessor" => {
                    input.parse::<Token![=]>()?;
                    accessor = Some(input.parse::<Ident>()?);
                }
                "no_accessor" => {
                    no_accessor = true;
                }
                "no_trait" => {
                    no_trait = true;
                }
                "also" => {
                    // also("path1", "path2", ...) — extra paths sharing this struct type.
                    let content;
                    syn::parenthesized!(content in input);
                    loop {
                        if content.is_empty() {
                            break;
                        }
                        extra_paths.push(content.parse::<LitStr>()?);
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument `{other}`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        let path =
            path.ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `path`"))?;

        // Compile-time validation of incompatible flag combinations.
        if id_type.is_some() && namespace {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`id_type` and `namespace` are mutually exclusive",
            ));
        }
        if id_type.is_some() && collection {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`id_type` and `collection` are mutually exclusive",
            ));
        }
        if (paged && id_type.is_none() && !collection) || (no_default_patch && id_type.is_none()) {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`paged` requires `id_type` or `collection`; `no_default_patch` requires `id_type`",
            ));
        }
        if paged && nullable {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`paged` and `nullable` are mutually exclusive",
            ));
        }
        if nullable && !collection {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`nullable` requires `collection`",
            ));
        }
        if namespace && collection {
            return Err(syn::Error::new(
                proc_macro2::Span::call_site(),
                "`namespace` and `collection` are mutually exclusive",
            ));
        }

        Ok(EndpointStructArgs {
            path,
            extra_paths,
            id_type,
            paged,
            no_default_patch,
            collection,
            nullable,
            namespace,
            no_registry,
            auth,
            accessor,
            no_accessor,
            no_trait,
        })
    }
}

/// Arguments for `#[gw2_endpoint(...)]` on an async fn.
struct EndpointFnArgs {
    path: LitStr,
    auth: bool,
    test_params: Vec<(Ident, Expr)>,
    no_test: bool,
}

impl Parse for EndpointFnArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut path = None;
        let mut auth = false;
        let mut test_params = Vec::new();
        let mut no_test = false;

        loop {
            if input.is_empty() {
                break;
            }
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "path" => {
                    input.parse::<Token![=]>()?;
                    path = Some(input.parse::<LitStr>()?);
                }
                "auth" => {
                    auth = true;
                }
                "no_test" => {
                    no_test = true;
                }
                "test_params" => {
                    let content;
                    syn::parenthesized!(content in input);
                    loop {
                        if content.is_empty() {
                            break;
                        }
                        let name: Ident = content.parse()?;
                        content.parse::<Token![=]>()?;
                        let value: Expr = content.parse()?;
                        test_params.push((name, value));
                        if !content.is_empty() {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                other => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown argument `{other}`"),
                    ));
                }
            }
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(EndpointFnArgs {
            path: path
                .ok_or_else(|| syn::Error::new(proc_macro2::Span::call_site(), "missing `path`"))?,
            auth,
            test_params,
            no_test,
        })
    }
}

// ── #[gw2_endpoint] ──────────────────────────────────────────────────────────

/// Unified endpoint annotation for all GW2 API endpoint patterns.
///
/// ## On a struct — Patterns 1 and 2
///
/// The pattern is inferred from attributes:
///
/// ### Pattern 1 — ID resource (`id_type` present)
/// ```rust,ignore
/// // Generates FooId(u32) newtype + FooEndpoint with get/get_many/list_ids/all/pages
/// #[gw2_endpoint(path = "items", id_type = u32, paged)]
/// pub struct Item { pub id: ItemId, ... }
///
/// // Reuse an existing ID type (no newtype generated):
/// #[gw2_endpoint(path = "commerce/prices", id_type = ItemId)]
/// pub struct ItemPrice { pub id: ItemId, ... }
/// ```
///
/// ### Pattern 2 — Fixed endpoint (no `id_type`)
/// ```rust,ignore
/// // Single object:
/// #[gw2_endpoint(path = "account", auth)]
/// pub struct Account { ... }
///
/// // Array of items (annotate the item type, not a marker):
/// #[gw2_endpoint(path = "account/wallet", auth, collection)]
/// pub struct WalletEntry { ... }
///
/// // Array of nullable slots:
/// #[gw2_endpoint(path = "account/bank", auth, collection, nullable)]
/// pub struct BankSlot { ... }
///
/// // Navigation handle only (no .get(), no registry):
/// #[gw2_endpoint(path = "commerce", namespace)]
/// pub struct Commerce;
/// ```
///
/// ## On an async fn — Pattern 3
///
/// The fn name and path together determine the parent endpoint:
/// - If fn name == last path segment → attach to second-to-last's endpoint.
/// - If fn name != last path segment → fn name is treated as an extra segment.
///
/// ```rust,ignore
/// // Attaches to ExchangeEndpoint (coins == last seg):
/// #[gw2_endpoint(path = "commerce/exchange/coins", test_params(quantity = 10000u64))]
/// pub async fn coins(&self, quantity: u64) -> Result<ExchangeRate, Gw2ApiError> {
///     self.0.request("/commerce/exchange/coins").param("quantity", quantity).send().await
/// }
///
/// // Attaches to SearchEndpoint (input != last seg "search"):
/// #[gw2_endpoint(path = "recipes/search", test_params(input = RecipeId(19976u32)))]
/// pub async fn input(&self, input: RecipeId) -> Result<Vec<Ref<Recipe>>, Gw2ApiError> {
///     self.0.request("/recipes/search").param("input", input).send().await
/// }
/// ```
#[proc_macro_attribute]
pub fn gw2_endpoint(args: TokenStream, input: TokenStream) -> TokenStream {
    // Try to parse as a struct/enum first, then as a function.
    let input2: TokenStream2 = input.clone().into();
    if let Ok(derive_input) = syn::parse2::<DeriveInput>(input2) {
        let args = parse_macro_input!(args as EndpointStructArgs);
        return expand_endpoint_struct(args, derive_input)
            .unwrap_or_else(|e| e.into_compile_error())
            .into();
    }
    // Fall through to function handling.
    let args = parse_macro_input!(args as EndpointFnArgs);
    let func = parse_macro_input!(input as ItemFn);
    expand_endpoint_fn(args, func)
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

// ── Struct branch ─────────────────────────────────────────────────────────────

fn expand_endpoint_struct(
    args: EndpointStructArgs,
    input: DeriveInput,
) -> syn::Result<TokenStream2> {
    let path_value = args.path.value();
    let (endpoint_name, accessor_method, parent_endpoint) =
        path_to_endpoint(&path_value, args.accessor.clone());

    // ── Pattern 2: namespace — navigation handle, optional registry entry ────
    if args.namespace {
        return expand_namespace_struct(
            &input,
            &endpoint_name,
            &accessor_method,
            &parent_endpoint,
            args.auth,
            args.no_registry,
            &args.path,
        );
    }

    // ── Pattern 1: ID resource ─────────────────────────────────────────────────
    if let Some(ref id_kind) = args.id_type {
        let path_str = args.path.clone();
        let id_kind = id_kind.clone();
        return expand_resource_struct(
            args,
            input,
            &endpoint_name,
            &accessor_method,
            &parent_endpoint,
            &id_kind,
            &path_str,
        );
    }

    // ── Pattern 2: fixed endpoint (singleton or collection) ────────────────────
    let struct_name = input.ident.clone();
    let path_str = args.path.clone();
    let extra_paths = args.extra_paths.clone();
    let auth = args.auth;
    let collection = args.collection;
    // With `collection`: the list is paginated; `.get()` fetches every page.
    let paged_collection = args.paged && args.collection;

    // When no_trait is set, derive the endpoint handle name from the full path
    // (e.g. "commerce/transactions/current/buys" → `CommerceTransactionsCurrentBuysEndpoint`)
    // to avoid name collisions when multiple annotations share the same last segment.
    let (endpoint_name, accessor_method, parent_endpoint) = if args.no_trait {
        let full_pascal: String = path_value.split('/').map(to_pascal_case).collect();
        let ep = format_ident!("{}Endpoint", full_pascal);
        let acc = accessor_method;
        let parent = parent_endpoint;
        (ep, acc, parent)
    } else {
        (endpoint_name, accessor_method, parent_endpoint)
    };

    let mut out = expand_fixed_endpoint_struct(
        args,
        input,
        &struct_name,
        &endpoint_name,
        &accessor_method,
        &parent_endpoint,
        &path_str,
    )?;

    // Generate extra endpoint handles + registry entries for additional paths
    // that share the same struct type (no struct/trait re-emission).
    for extra_path_lit in &extra_paths {
        let extra_path_value = extra_path_lit.value();
        let (_, extra_accessor, extra_parent) = path_to_endpoint(&extra_path_value, None);
        let full_pascal: String = extra_path_value.split('/').map(to_pascal_case).collect();
        let extra_ep = format_ident!("{}Endpoint", full_pascal);
        let path_with_slash = format!("/{}", extra_path_value);
        let path_with_slash_lit = syn::LitStr::new(&path_with_slash, extra_path_lit.span());
        let type_name_str = format!("Vec<{}>", struct_name);
        let registry_static = format_ident!(
            "_GW2_REGISTRY_PATH_{}_{}",
            extra_path_value.replace('/', "_").to_uppercase(),
            struct_name.to_string().to_uppercase()
        );
        let auth_bool = auth;
        let call_str = accessor_chain(&extra_path_value, Some(("get", "")));

        let (single_body, full_body): (TokenStream2, TokenStream2) = if auth_bool {
            (
                quote! {
                    match auth {
                        ::std::option::Option::Some(client) => {
                            client.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await
                                .map(|_| ())
                                .map_err(|e| format!("fetch: {e}"))
                        }
                        ::std::option::Option::None => ::std::result::Result::Ok(()),
                    }
                },
                quote! {
                    let client = match auth {
                        ::std::option::Option::Some(c) => c,
                        ::std::option::Option::None => return ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec::Vec::new(),
                        },
                    };
                    match client.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await {
                        ::std::result::Result::Ok(items) => ::gw2_api::registry::FullFetchResult {
                            ok_count: items.len(),
                            errors: ::std::vec::Vec::new(),
                        },
                        ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                        },
                    }
                },
            )
        } else {
            (
                quote! {
                    unauth.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await
                        .map(|_| ())
                        .map_err(|e| format!("fetch: {e}"))
                },
                quote! {
                    match unauth.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await {
                        ::std::result::Result::Ok(items) => ::gw2_api::registry::FullFetchResult {
                            ok_count: items.len(),
                            errors: ::std::vec::Vec::new(),
                        },
                        ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                        },
                    }
                },
            )
        };

        let accessor_impl: TokenStream2 = match (&extra_parent, auth_bool) {
            (Some(parent), true) => quote! {
                impl<'c> #parent<'c, ::gw2_api::client::auth::Authenticated> {
                    pub fn #extra_accessor(&self) -> #extra_ep<'c, ::gw2_api::client::auth::Authenticated> {
                        #extra_ep(self.0)
                    }
                }
            },
            (Some(parent), false) => quote! {
                impl<'c, S: ::gw2_api::client::auth::AuthState> #parent<'c, S> {
                    pub fn #extra_accessor(&self) -> #extra_ep<'c, S> {
                        #extra_ep(self.0)
                    }
                }
            },
            (None, true) => quote! {
                impl ::gw2_api::client::Gw2Client<::gw2_api::client::auth::Authenticated> {
                    pub fn #extra_accessor(&self) -> #extra_ep<'_, ::gw2_api::client::auth::Authenticated> {
                        #extra_ep(self)
                    }
                }
            },
            (None, false) => quote! {
                impl<S: ::gw2_api::client::auth::AuthState> ::gw2_api::client::Gw2Client<S> {
                    pub fn #extra_accessor(&self) -> #extra_ep<'_, S> {
                        #extra_ep(self)
                    }
                }
            },
        };

        let extra_shape = if paged_collection {
            quote! { ::gw2_api::registry::Shape::Pages }
        } else {
            quote! { ::gw2_api::registry::Shape::Array }
        };
        let extra_fetch = if paged_collection {
            quote! { self.0.fetch_all_pages::<#struct_name>(#path_with_slash_lit).await }
        } else {
            quote! { self.0.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await }
        };
        let get_method: TokenStream2 = if auth_bool {
            quote! {
                impl<'c> #extra_ep<'c, ::gw2_api::client::auth::Authenticated> {
                    pub async fn get(&self) -> ::std::result::Result<::std::vec::Vec<#struct_name>, ::gw2_api::error::Gw2ApiError> {
                        #extra_fetch
                    }
                }
            }
        } else {
            quote! {
                impl<'c, S: ::gw2_api::client::auth::AuthState> #extra_ep<'c, S> {
                    pub async fn get(&self) -> ::std::result::Result<::std::vec::Vec<#struct_name>, ::gw2_api::error::Gw2ApiError> {
                        #extra_fetch
                    }
                }
            }
        };

        out.extend(quote! {
            #[::linkme::distributed_slice(::gw2_api::registry::ENDPOINTS)]
            static #registry_static: ::gw2_api::registry::EndpointEntry = ::gw2_api::registry::EndpointEntry {
                path: #extra_path_lit,
                auth: #auth_bool,
                type_name: #type_name_str,
                call: #call_str,
                check: ::std::option::Option::Some(::gw2_api::registry::CheckSpec {
                    shape: #extra_shape,
                    parse: ::gw2_api::registry::parse_value::<#struct_name>,
                }),
                single: |unauth, auth| {
                    ::std::boxed::Box::pin(async move {
                        #single_body
                    })
                },
                full: |unauth, auth| {
                    ::std::boxed::Box::pin(async move {
                        #full_body
                    })
                },
            };

            pub struct #extra_ep<'c, S: ::gw2_api::client::auth::AuthState>(
                pub(crate) &'c ::gw2_api::client::Gw2Client<S>,
            );

            #get_method
            #accessor_impl
        });
    }

    // Suppress unused variable warning: collection is checked indirectly via extra_paths usage.
    let _ = collection;

    Ok(out)
}

/// Pattern 2 — namespace: generates a navigation handle + accessor.
///
/// Unless `no_registry` is set, also registers an `EndpointEntry` whose `single`
/// closure fetches the path and deserializes the `Vec<String>` sub-endpoint listing
/// that the GW2 API returns for navigation endpoints.
fn expand_namespace_struct(
    input: &DeriveInput,
    endpoint_name: &Ident,
    accessor_method: &Ident,
    parent_endpoint: &Option<Ident>,
    auth: bool,
    no_registry: bool,
    path_str: &LitStr,
) -> syn::Result<TokenStream2> {
    let struct_def = quote! {
        pub struct #endpoint_name<'c, S: ::gw2_api::client::auth::AuthState>(
            pub(crate) &'c ::gw2_api::client::Gw2Client<S>,
        );
    };

    let accessor_impl = match (parent_endpoint, auth) {
        (Some(parent), _) => quote! {
            impl<'c, S: ::gw2_api::client::auth::AuthState> #parent<'c, S> {
                pub fn #accessor_method(&self) -> #endpoint_name<'c, S> {
                    #endpoint_name(self.0)
                }
            }
        },
        (None, true) => quote! {
            impl ::gw2_api::client::Gw2Client<::gw2_api::client::auth::Authenticated> {
                pub fn #accessor_method(&self) -> #endpoint_name<'_, ::gw2_api::client::auth::Authenticated> {
                    #endpoint_name(self)
                }
            }
        },
        (None, false) => quote! {
            impl<S: ::gw2_api::client::auth::AuthState> ::gw2_api::client::Gw2Client<S> {
                pub fn #accessor_method(&self) -> #endpoint_name<'_, S> {
                    #endpoint_name(self)
                }
            }
        },
    };

    // Optionally emit a registry entry that fetches the Vec<String> sub-listing.
    let registry_entry = if no_registry {
        quote! {}
    } else {
        let auth_bool = auth;
        // Use the endpoint struct name to get a unique static name per entry.
        let static_name = format_ident!("_ENDPOINT_{}", endpoint_name.to_string().to_uppercase());
        // Request paths are relative to the base URL and need the leading slash.
        let request_path = syn::LitStr::new(&format!("/{}", path_str.value()), path_str.span());
        let single_body: TokenStream2 = if auth_bool {
            quote! {
                match auth {
                    ::std::option::Option::Some(client) => {
                        client.request(#request_path).send::<::std::vec::Vec<::std::string::String>>().await
                            .map(|_| ())
                            .map_err(|e| ::std::format!("fetch: {e}"))
                    }
                    ::std::option::Option::None => ::std::result::Result::Ok(()),
                }
            }
        } else {
            quote! {
                unauth.request(#request_path).send::<::std::vec::Vec<::std::string::String>>().await
                    .map(|_| ())
                    .map_err(|e| ::std::format!("fetch: {e}"))
            }
        };
        let call_str = accessor_chain(&path_str.value(), None);
        let handle_type_str = endpoint_name.to_string();
        quote! {
            #[::linkme::distributed_slice(::gw2_api::registry::ENDPOINTS)]
            static #static_name: ::gw2_api::registry::EndpointEntry = ::gw2_api::registry::EndpointEntry {
                path: #path_str,
                auth: #auth_bool,
                type_name: #handle_type_str,
                call: #call_str,
                check: ::std::option::Option::None,
                single: |unauth, auth| {
                    ::std::boxed::Box::pin(async move {
                        #single_body
                    })
                },
                full: |_unauth, _auth| {
                    ::std::boxed::Box::pin(async move {
                        ::gw2_api::registry::FullFetchResult { ok_count: 0, errors: ::std::vec::Vec::new() }
                    })
                },
            };
        }
    };

    // Discard the user's struct body — we only use the annotation.
    let _ = input;
    Ok(quote! {
        #struct_def
        #accessor_impl
        #registry_entry
    })
}

/// Pattern 1 — ID resource: generates newtype ID (if needed), endpoint handle,
/// bulk ops, and registry entry.
fn expand_resource_struct(
    args: EndpointStructArgs,
    input: DeriveInput,
    endpoint_name: &Ident,
    accessor_method: &Ident,
    parent_endpoint: &Option<Ident>,
    id_kind: &IdType,
    path_str: &LitStr,
) -> syn::Result<TokenStream2> {
    let struct_name = &input.ident;
    let id_type_ident = id_kind.as_ident(struct_name);
    let auth = args.auth;
    // `auth` resources are only reachable from an authenticated client.
    let (state_generics, state_ty) = if auth {
        (quote! {}, quote! { ::gw2_api::client::auth::Authenticated })
    } else {
        (quote! { S: ::gw2_api::client::auth::AuthState }, quote! { S })
    };
    // The client the registry smoke tests call through.
    let (registry_single_client, registry_full_client) = if auth {
        (
            quote! {
                let client = auth.ok_or_else(|| ::std::string::String::from("requires an API key"))?;
            },
            quote! {
                let ::std::option::Option::Some(client) = auth else {
                    return ::gw2_api::registry::FullFetchResult {
                        ok_count: 0,
                        errors: ::std::vec![(::std::string::String::from("auth"), ::std::string::String::from("requires an API key"))],
                    };
                };
            },
        )
    } else {
        (quote! { let client = unauth; }, quote! { let client = unauth; })
    };

    // Optionally generate the newtype ID.
    let id_newtype = match id_kind {
        IdType::Generate(primitive) => {
            let id_name = &id_type_ident;
            let primitive_str = primitive.to_string();

            // Visitor methods differ based on whether the primitive is a string or integer.
            let scalar_visitors = if primitive_str == "String" {
                quote! {
                    fn visit_str<E: ::serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                        Ok(#id_name(v.to_owned()))
                    }
                    fn visit_string<E: ::serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                        Ok(#id_name(v))
                    }
                }
            } else {
                quote! {
                    fn visit_u64<E: ::serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                        Ok(#id_name(v as #primitive))
                    }
                }
            };

            quote! {
                #[derive(Debug, Clone, PartialEq, Eq, Hash, ::serde::Serialize)]
                #[serde(transparent)]
                pub struct #id_name(pub #primitive);

                impl<'de> ::serde::Deserialize<'de> for #id_name {
                    fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                        struct Visitor;
                        impl<'de> ::serde::de::Visitor<'de> for Visitor {
                            type Value = #id_name;
                            fn expecting(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                                write!(f, "a {} or an object with an \"id\" field", stringify!(#primitive))
                            }
                            #scalar_visitors
                            fn visit_map<A: ::serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                                let mut id: Option<#primitive> = None;
                                while let Some(key) = map.next_key::<String>()? {
                                    if key == "id" {
                                        id = Some(map.next_value()?);
                                    } else {
                                        map.next_value::<::serde::de::IgnoredAny>()?;
                                    }
                                }
                                let id = id.ok_or_else(|| ::serde::de::Error::missing_field("id"))?;
                                Ok(#id_name(id))
                            }
                        }
                        deserializer.deserialize_any(Visitor)
                    }
                }

                impl ::std::fmt::Display for #id_name {
                    fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                        write!(f, "{}", self.0)
                    }
                }
                impl ::std::convert::From<#primitive> for #id_name {
                    fn from(v: #primitive) -> Self { Self(v) }
                }
                impl ::std::convert::From<#id_name> for #primitive {
                    fn from(v: #id_name) -> Self { v.0 }
                }
                impl ::gw2_api::resource::ResourceId for #id_name {}
                impl #id_name {
                    pub fn get<#state_generics>(
                        &self,
                        client: &::gw2_api::client::Gw2Client<#state_ty>,
                    ) -> impl ::std::future::Future<Output = ::std::result::Result<#struct_name, ::gw2_api::error::Gw2ApiError>> + Send
                    where
                        #struct_name: ::gw2_api::resource::Patchable,
                    {
                        let id = self.clone();
                        async move { client.get::<#struct_name>(id).await }
                    }
                }
            }
        }
        IdType::Existing(_) => quote! {},
    };

    let patchable_impl = if args.no_default_patch {
        quote! {}
    } else {
        quote! { impl ::gw2_api::resource::Patchable for #struct_name {} }
    };

    let paged_impl = if args.paged {
        quote! { impl ::gw2_api::resource::PagedResource for #struct_name {} }
    } else {
        quote! {}
    };

    let pages_method = if args.paged {
        quote! {
            /// Stream all resources page by page.
            pub fn pages(
                &self,
                opts: ::gw2_api::resource::PageOptions,
            ) -> impl ::futures::Stream<Item = ::std::result::Result<#struct_name, ::gw2_api::error::Gw2ApiError>> + '_ {
                self.0.pages::<#struct_name>(opts)
            }

            /// Fetch all resources. Tries `?ids=all` first, falls back to pagination.
            pub async fn all(&self) -> ::std::result::Result<::std::vec::Vec<#struct_name>, ::gw2_api::error::Gw2ApiError> {
                self.0.all::<#struct_name>().await
            }
        }
    } else {
        quote! {}
    };

    let type_name_str = struct_name.to_string();
    let registry_static = format_ident!("_GW2_REGISTRY_{}", struct_name.to_string().to_uppercase());

    // `full` for paged resources uses pages() (same bulk deserialization as the real lib).
    // Non-paged resources fall back to list_ids() + get_many().
    let resource_full_body: TokenStream2 = if args.paged {
        quote! {
            use ::futures::StreamExt as _;
            let mut ok_count: usize = 0;
            let mut errors: ::std::vec::Vec<(::std::string::String, ::std::string::String)> = ::std::vec::Vec::new();
            let opts = ::gw2_api::resource::PageOptions::default().page_size(200);
            let mut stream = ::std::pin::pin!(client.pages::<#struct_name>(opts));
            let mut idx: usize = 0;
            while let ::std::option::Option::Some(result) = stream.next().await {
                match result {
                    ::std::result::Result::Ok(_) => ok_count += 1,
                    ::std::result::Result::Err(e) => {
                        errors.push((format!("item:{idx}"), e.to_string()));
                    }
                }
                idx += 1;
            }
            ::gw2_api::registry::FullFetchResult { ok_count, errors }
        }
    } else {
        quote! {
            let ids = match client.list_ids::<#struct_name>().await {
                ::std::result::Result::Ok(v) => v,
                ::std::result::Result::Err(e) => return ::gw2_api::registry::FullFetchResult {
                    ok_count: 0,
                    errors: ::std::vec![(::std::string::String::from("list_ids"), e.to_string())],
                },
            };
            match client.get_many::<#struct_name>(ids).await {
                ::std::result::Result::Ok(items) => ::gw2_api::registry::FullFetchResult {
                    ok_count: items.len(),
                    errors: ::std::vec::Vec::new(),
                },
                ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                    ok_count: 0,
                    errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                },
            }
        }
    };

    let accessor_impl = if args.no_accessor {
        quote! {}
    } else {
        match parent_endpoint {
            Some(parent) => quote! {
                impl<'c, #state_generics> #parent<'c, #state_ty> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'_, #state_ty> {
                        #endpoint_name(self.0)
                    }
                }
            },
            None => quote! {
                impl<#state_generics> ::gw2_api::client::Gw2Client<#state_ty> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'_, #state_ty> {
                        #endpoint_name(self)
                    }
                }
            },
        }
    };

    let call_str = accessor_chain(&path_str.value(), Some(("get", "id")));

    Ok(quote! {
        #input

        #id_newtype

        impl ::gw2_api::resource::Resource for #struct_name {
            type Id = #id_type_ident;
            const PATH: &'static str = #path_str;
            const AUTH: bool = #auth;

            fn id(&self) -> &Self::Id {
                &self.id
            }
        }

        #patchable_impl
        #paged_impl

        #[::linkme::distributed_slice(::gw2_api::registry::ENDPOINTS)]
        static #registry_static: ::gw2_api::registry::EndpointEntry = ::gw2_api::registry::EndpointEntry {
            path: #path_str,
            auth: #auth,
            type_name: #type_name_str,
            call: #call_str,
            check: ::std::option::Option::Some(::gw2_api::registry::CheckSpec {
                    shape: ::gw2_api::registry::Shape::Bulk,
                    parse: ::gw2_api::registry::parse_value::<#struct_name>,
                }),
            single: |unauth, auth| {
                ::std::boxed::Box::pin(async move {
                    #registry_single_client
                    let ids = client.list_ids::<#struct_name>().await
                        .map_err(|e| format!("list_ids: {e}"))?;
                    let id = ids.into_iter().next()
                        .ok_or_else(|| ::std::string::String::from("endpoint returned no IDs"))?;
                    client.get::<#struct_name>(id).await
                        .map(|_| ())
                        .map_err(|e| format!("get: {e}"))
                })
            },
            full: |unauth, auth| {
                ::std::boxed::Box::pin(async move {
                    #registry_full_client
                    #resource_full_body
                })
            },
        };

        pub struct #endpoint_name<'c, S: ::gw2_api::client::auth::AuthState>(
            pub(crate) &'c ::gw2_api::client::Gw2Client<S>,
        );

        impl<'c, #state_generics> #endpoint_name<'c, #state_ty> {
            /// Fetch a single resource by ID.
            pub async fn get(
                &self,
                id: impl ::std::convert::Into<#id_type_ident>,
            ) -> ::std::result::Result<#struct_name, ::gw2_api::error::Gw2ApiError> {
                self.0.get::<#struct_name>(id).await
            }

            /// Fetch multiple resources by IDs (auto-chunked at 200).
            pub async fn get_many(
                &self,
                ids: impl ::std::iter::IntoIterator<Item = #id_type_ident>,
            ) -> ::std::result::Result<::std::vec::Vec<#struct_name>, ::gw2_api::error::Gw2ApiError> {
                self.0.get_many::<#struct_name>(ids).await
            }

            /// List all IDs for this resource.
            pub async fn list_ids(
                &self,
            ) -> ::std::result::Result<::std::vec::Vec<#id_type_ident>, ::gw2_api::error::Gw2ApiError> {
                self.0.list_ids::<#struct_name>().await
            }

            #pages_method
        }

        #accessor_impl
    })
}

/// Pattern 2 — fixed endpoint (singleton or collection_singleton):
/// generates endpoint handle, .get() returning `Self` or `Vec<Self>`/`Vec<Option<Self>>`,
/// and registry entry.
fn expand_fixed_endpoint_struct(
    args: EndpointStructArgs,
    input: DeriveInput,
    struct_name: &Ident,
    endpoint_name: &Ident,
    accessor_method: &Ident,
    parent_endpoint: &Option<Ident>,
    path_str: &LitStr,
) -> syn::Result<TokenStream2> {
    let auth = args.auth;
    let collection = args.collection;
    let nullable = args.nullable;
    let no_trait = args.no_trait;

    // Build the path string with a leading "/" for direct request calls.
    let path_with_slash = format!("/{}", path_str.value());
    let path_with_slash_lit = syn::LitStr::new(&path_with_slash, path_str.span());

    // The return type of .get() and the serde target type.
    // When no_trait: no trait impl is emitted (shared struct, only extra registry+handle).
    let (get_return_type, trait_impl) = if no_trait {
        // no_trait only makes sense with collection — return Vec<struct_name>.
        let return_ty = quote! { ::std::vec::Vec<#struct_name> };
        (return_ty, quote! {})
    } else if collection {
        let item_ty: TokenStream2 = if nullable {
            quote! { ::std::option::Option<#struct_name> }
        } else {
            quote! { #struct_name }
        };
        let return_ty = quote! { ::std::vec::Vec<#item_ty> };
        let trait_impl = quote! {
            impl ::gw2_api::resource::CollectionSingletonResource for #struct_name {
                type Item = #item_ty;
                const PATH: &'static str = #path_str;
            }
        };
        (return_ty, trait_impl)
    } else {
        let return_ty = quote! { #struct_name };
        let trait_impl = quote! {
            impl ::gw2_api::resource::SingletonResource for #struct_name {
                const PATH: &'static str = #path_str;
            }
        };
        (return_ty, trait_impl)
    };

    // The inner fetch call for .get(). When no_trait, inline the request directly.
    let paged_collection = args.paged && collection;
    let fetch_call = if paged_collection {
        quote! { self.0.fetch_all_pages::<#struct_name>(#path_with_slash_lit).await }
    } else if no_trait {
        quote! { self.0.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await }
    } else if collection {
        quote! { self.0.fetch_collection::<#struct_name>().await }
    } else {
        quote! { self.0.fetch_singleton::<#struct_name>().await }
    };

    let check_shape = if paged_collection {
        quote! { ::gw2_api::registry::Shape::Pages }
    } else if collection {
        quote! { ::gw2_api::registry::Shape::Array }
    } else {
        quote! { ::gw2_api::registry::Shape::One }
    };
    let check_spec = quote! {
        ::gw2_api::registry::CheckSpec {
            shape: #check_shape,
            parse: ::gw2_api::registry::parse_value::<#struct_name>,
        }
    };
    let type_name_str = if collection {
        if nullable {
            format!("Vec<Option<{}>>", struct_name)
        } else {
            format!("Vec<{}>", struct_name)
        }
    } else {
        struct_name.to_string()
    };
    let registry_static = format_ident!(
        "_GW2_REGISTRY_PATH_{}_{}",
        path_str.value().replace('/', "_").to_uppercase(),
        struct_name.to_string().to_uppercase()
    );
    let auth_bool = auth;

    // Generate single/full closure bodies, branching on collection vs singleton.
    // Each uses the real Gw2Client methods so the validation path matches the lib.
    // When no_trait, inline the request directly since fetch_collection uses a trait const.
    let single_body: TokenStream2 = if no_trait || collection {
        if auth_bool {
            let fetch = if no_trait {
                quote! { client.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await }
            } else {
                quote! { client.fetch_collection::<#struct_name>().await }
            };
            quote! {
                match auth {
                    ::std::option::Option::Some(client) => {
                        #fetch
                            .map(|_| ())
                            .map_err(|e| format!("fetch: {e}"))
                    }
                    ::std::option::Option::None => ::std::result::Result::Ok(()),
                }
            }
        } else {
            if no_trait {
                quote! {
                    unauth.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await
                        .map(|_| ())
                        .map_err(|e| format!("fetch: {e}"))
                }
            } else {
                quote! {
                    unauth.fetch_collection::<#struct_name>().await
                        .map(|_| ())
                        .map_err(|e| format!("fetch: {e}"))
                }
            }
        }
    } else if auth_bool {
        quote! {
            match auth {
                ::std::option::Option::Some(client) => {
                    client.fetch_singleton::<#struct_name>().await
                        .map(|_| ())
                        .map_err(|e| format!("fetch: {e}"))
                }
                ::std::option::Option::None => ::std::result::Result::Ok(()),
            }
        }
    } else {
        quote! {
            unauth.fetch_singleton::<#struct_name>().await
                .map(|_| ())
                .map_err(|e| format!("fetch: {e}"))
        }
    };

    let full_body: TokenStream2 = if no_trait || collection {
        if auth_bool {
            let fetch = if no_trait {
                quote! { client.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await }
            } else {
                quote! { client.fetch_collection::<#struct_name>().await }
            };
            quote! {
                let client = match auth {
                    ::std::option::Option::Some(c) => c,
                    ::std::option::Option::None => return ::gw2_api::registry::FullFetchResult {
                        ok_count: 0,
                        errors: ::std::vec::Vec::new(),
                    },
                };
                match #fetch {
                    ::std::result::Result::Ok(items) => ::gw2_api::registry::FullFetchResult {
                        ok_count: items.len(),
                        errors: ::std::vec::Vec::new(),
                    },
                    ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                        ok_count: 0,
                        errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                    },
                }
            }
        } else {
            let fetch = if no_trait {
                quote! { unauth.request(#path_with_slash_lit).send::<::std::vec::Vec<#struct_name>>().await }
            } else {
                quote! { unauth.fetch_collection::<#struct_name>().await }
            };
            quote! {
                match #fetch {
                    ::std::result::Result::Ok(items) => ::gw2_api::registry::FullFetchResult {
                        ok_count: items.len(),
                        errors: ::std::vec::Vec::new(),
                    },
                    ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                        ok_count: 0,
                        errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                    },
                }
            }
        }
    } else if auth_bool {
        quote! {
            let client = match auth {
                ::std::option::Option::Some(c) => c,
                ::std::option::Option::None => return ::gw2_api::registry::FullFetchResult {
                    ok_count: 0,
                    errors: ::std::vec::Vec::new(),
                },
            };
            match client.fetch_singleton::<#struct_name>().await {
                ::std::result::Result::Ok(_) => ::gw2_api::registry::FullFetchResult {
                    ok_count: 1,
                    errors: ::std::vec::Vec::new(),
                },
                ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                    ok_count: 0,
                    errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                },
            }
        }
    } else {
        quote! {
            match unauth.fetch_singleton::<#struct_name>().await {
                ::std::result::Result::Ok(_) => ::gw2_api::registry::FullFetchResult {
                    ok_count: 1,
                    errors: ::std::vec::Vec::new(),
                },
                ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                    ok_count: 0,
                    errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                },
            }
        }
    };

    // Accessor: auth endpoints → lock to Authenticated; public → generic S.
    let accessor_impl = if args.no_accessor {
        quote! {}
    } else {
        match (parent_endpoint, auth) {
            (Some(parent), true) => quote! {
                impl<'c> #parent<'c, ::gw2_api::client::auth::Authenticated> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'c, ::gw2_api::client::auth::Authenticated> {
                        #endpoint_name(self.0)
                    }
                }
            },
            (Some(parent), false) => quote! {
                impl<'c, S: ::gw2_api::client::auth::AuthState> #parent<'c, S> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'c, S> {
                        #endpoint_name(self.0)
                    }
                }
            },
            (None, true) => quote! {
                impl ::gw2_api::client::Gw2Client<::gw2_api::client::auth::Authenticated> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'_, ::gw2_api::client::auth::Authenticated> {
                        #endpoint_name(self)
                    }
                }
            },
            (None, false) => quote! {
                impl<S: ::gw2_api::client::auth::AuthState> ::gw2_api::client::Gw2Client<S> {
                    pub fn #accessor_method(&self) -> #endpoint_name<'_, S> {
                        #endpoint_name(self)
                    }
                }
            },
        }
    };

    // The .get() method is locked to Authenticated if auth, generic S otherwise.
    let get_method = if auth {
        quote! {
            impl<'c> #endpoint_name<'c, ::gw2_api::client::auth::Authenticated> {
                /// Fetch the resource (requires authentication).
                pub async fn get(&self) -> ::std::result::Result<#get_return_type, ::gw2_api::error::Gw2ApiError> {
                    #fetch_call
                }
            }
        }
    } else {
        quote! {
            impl<'c, S: ::gw2_api::client::auth::AuthState> #endpoint_name<'c, S> {
                /// Fetch the resource.
                pub async fn get(&self) -> ::std::result::Result<#get_return_type, ::gw2_api::error::Gw2ApiError> {
                    #fetch_call
                }
            }
        }
    };

    let call_str = accessor_chain(&path_str.value(), Some(("get", "")));

    // When no_trait: only emit the endpoint handle + registry entry (no struct, no trait impl).
    let struct_and_trait = if no_trait {
        quote! {}
    } else {
        quote! {
            #input
            #trait_impl
        }
    };

    Ok(quote! {
        #struct_and_trait

        #[::linkme::distributed_slice(::gw2_api::registry::ENDPOINTS)]
        static #registry_static: ::gw2_api::registry::EndpointEntry = ::gw2_api::registry::EndpointEntry {
            path: #path_str,
            auth: #auth_bool,
            type_name: #type_name_str,
            call: #call_str,
            check: ::std::option::Option::Some(#check_spec),
            single: |unauth, auth| {
                ::std::boxed::Box::pin(async move {
                    #single_body
                })
            },
            full: |unauth, auth| {
                ::std::boxed::Box::pin(async move {
                    #full_body
                })
            },
        };

        pub struct #endpoint_name<'c, S: ::gw2_api::client::auth::AuthState>(
            pub(crate) &'c ::gw2_api::client::Gw2Client<S>,
        );

        #get_method

        #accessor_impl
    })
}

// ── Function branch ───────────────────────────────────────────────────────────

fn expand_endpoint_fn(args: EndpointFnArgs, func: ItemFn) -> syn::Result<TokenStream2> {
    let path_str = &args.path;
    let path_value = args.path.value();
    let auth_bool = args.auth;
    let fn_name = &func.sig.ident;

    let (parent_endpoint, _method_name) = fn_parent_from_path(&path_value, fn_name);

    // Extract the inner type from `Result<T, E>` for display in the registry.
    let type_name_str = match &func.sig.output {
        syn::ReturnType::Type(_, ty) => {
            // Stringify and strip outer Result<..., ...> wrapper if present.
            let raw = quote::quote!(#ty).to_string();
            // raw looks like "Result < ExchangeRate , Gw2ApiError >"
            // Extract the first type argument.
            if let Some(inner) = raw.strip_prefix("Result <") {
                inner
                    .trim()
                    .split(',')
                    .next()
                    .unwrap_or(inner)
                    .trim()
                    .replace(" ", "")
            } else {
                raw.replace(" ", "")
            }
        }
        syn::ReturnType::Default => fn_name.to_string(),
    };
    let registry_static = format_ident!(
        "_GW2_REGISTRY_PATH_{}_FN_{}",
        path_value.replace("/", "_").to_uppercase(),
        fn_name.to_string().to_uppercase()
    );

    // Collect non-self parameter names for test_params validation.
    let param_names: Vec<&Ident> = func
        .sig
        .inputs
        .iter()
        .filter_map(|arg| {
            if let FnArg::Typed(pat_type) = arg
                && let syn::Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                    return Some(&pat_ident.ident);
                }
            None
        })
        .collect();

    // Build full accessor chain, e.g. "client.recipes().search().input(input)".
    let args_str = param_names
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let last_seg = path_value.split('/').next_back().unwrap_or("");
    let fn_str = fn_name.to_string();
    // Build chain: if fn_name differs from last path segment, the full path is
    // the handle chain and fn_name is the terminal call. Otherwise strip the last
    // segment from the path (it IS the fn_name) and use fn_name as terminal.
    let call_sig_str = if fn_str != last_seg {
        // path="recipes/search", fn="input" → ".recipes().search().input(input)"
        accessor_chain(&path_value, Some((&fn_str, &args_str)))
    } else {
        // path="commerce/exchange/coins", fn="coins" → ".commerce().exchange().coins(quantity)"
        let parent_path = path_value.rsplit_once('/').map(|x| x.0).unwrap_or("");
        if parent_path.is_empty() {
            // Top-level fn, e.g. path="createsubtoken", fn="createsubtoken"
            format!(".{}({})", fn_str, args_str)
        } else {
            accessor_chain(parent_path, Some((&fn_str, &args_str)))
        }
    };

    // Validate that each non-self param has a test_params entry (unless no_test).
    let test_param_names: Vec<&Ident> = args.test_params.iter().map(|(n, _)| n).collect();
    if !args.no_test {
        for pname in &param_names {
            if !test_param_names
                .iter()
                .any(|t| t.to_string() == pname.to_string())
            {
                return Err(syn::Error::new_spanned(
                    &func.sig,
                    format!(
                        "missing test_params value for parameter `{pname}` — add `test_params({pname} = ...)` or use `no_test`"
                    ),
                ));
            }
        }
    }

    // Build test param assignments and call args.
    let test_assignments: Vec<_> = args
        .test_params
        .iter()
        .map(|(name, value)| {
            quote! { let #name = #value; }
        })
        .collect();

    // Build the single/full closure bodies for fn endpoints.
    // These call the actual method through the real Gw2Client endpoint handle.
    // The parent endpoint struct is known at expansion time.
    let (single_body, full_body) = if args.no_test {
        (
            quote! { ::std::boxed::Box::pin(async move { ::std::result::Result::Ok(()) }) },
            quote! { ::std::boxed::Box::pin(async move {
                ::gw2_api::registry::FullFetchResult { ok_count: 0, errors: ::std::vec::Vec::new() }
            }) },
        )
    } else {
        // Build the endpoint handle expression. If the fn is on a root endpoint
        // (no parent), call directly on the client. Otherwise construct ParentEndpoint(&client).
        let handle_expr: TokenStream2 = match &parent_endpoint {
            Some(parent) => quote! { #parent(&client) },
            None => quote! { client },
        };
        let single = if auth_bool {
            quote! {
                ::std::boxed::Box::pin(async move {
                    let client = match auth {
                        ::std::option::Option::Some(c) => c,
                        ::std::option::Option::None => return ::std::result::Result::Ok(()),
                    };
                    #(#test_assignments)*
                    #handle_expr.#fn_name(#(#test_param_names),*).await
                        .map(|_| ())
                        .map_err(|e| format!("{e}"))
                })
            }
        } else {
            quote! {
                ::std::boxed::Box::pin(async move {
                    let client = unauth;
                    #(#test_assignments)*
                    #handle_expr.#fn_name(#(#test_param_names),*).await
                        .map(|_| ())
                        .map_err(|e| format!("{e}"))
                })
            }
        };
        let full = if auth_bool {
            quote! {
                ::std::boxed::Box::pin(async move {
                    let client = match auth {
                        ::std::option::Option::Some(c) => c,
                        ::std::option::Option::None => return ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec::Vec::new(),
                        },
                    };
                    #(#test_assignments)*
                    match #handle_expr.#fn_name(#(#test_param_names),*).await {
                        ::std::result::Result::Ok(_) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 1,
                            errors: ::std::vec::Vec::new(),
                        },
                        ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                        },
                    }
                })
            }
        } else {
            quote! {
                ::std::boxed::Box::pin(async move {
                    let client = unauth;
                    #(#test_assignments)*
                    match #handle_expr.#fn_name(#(#test_param_names),*).await {
                        ::std::result::Result::Ok(_) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 1,
                            errors: ::std::vec::Vec::new(),
                        },
                        ::std::result::Result::Err(e) => ::gw2_api::registry::FullFetchResult {
                            ok_count: 0,
                            errors: ::std::vec![(::std::string::String::from(#type_name_str), e.to_string())],
                        },
                    }
                })
            }
        };
        (single, full)
    };

    let impl_block = match (parent_endpoint, auth_bool) {
        (Some(parent), true) => quote! {
            impl<'c> #parent<'c, ::gw2_api::client::auth::Authenticated> {
                #func
            }
        },
        (Some(parent), false) => quote! {
            impl<'c, S: ::gw2_api::client::auth::AuthState> #parent<'c, S> {
                #func
            }
        },
        (None, true) => quote! {
            impl ::gw2_api::client::Gw2Client<::gw2_api::client::auth::Authenticated> {
                #func
            }
        },
        (None, false) => quote! {
            impl<S: ::gw2_api::client::auth::AuthState> ::gw2_api::client::Gw2Client<S> {
                #func
            }
        },
    };

    Ok(quote! {
        #impl_block

        #[::linkme::distributed_slice(::gw2_api::registry::ENDPOINTS)]
        static #registry_static: ::gw2_api::registry::EndpointEntry = ::gw2_api::registry::EndpointEntry {
            path: #path_str,
            auth: #auth_bool,
            type_name: #type_name_str,
            call: #call_sig_str,
            check: ::std::option::Option::None,
            single: |unauth, auth| { #single_body },
            full: |unauth, auth| { #full_body },
        };
    })
}

// ── Retired macros — compile errors pointing to gw2_endpoint ─────────────────

#[proc_macro_attribute]
pub fn gw2_resource(_args: TokenStream, _input: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[gw2_resource]` has been replaced by `#[gw2_endpoint(path = \"...\", id_type = u32)]`",
    )
    .into_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn gw2_singleton(_args: TokenStream, _input: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[gw2_singleton]` has been replaced by `#[gw2_endpoint(path = \"...\", auth)]`",
    )
    .into_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn gw2_collection_singleton(_args: TokenStream, _input: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[gw2_collection_singleton]` has been replaced by `#[gw2_endpoint(path = \"...\", collection)]` on the item type",
    ).into_compile_error().into()
}

#[proc_macro_attribute]
pub fn gw2_namespace(_args: TokenStream, _input: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[gw2_namespace]` has been replaced by `#[gw2_endpoint(path = \"...\", namespace)]`",
    )
    .into_compile_error()
    .into()
}

#[proc_macro_attribute]
pub fn gw2_method(_args: TokenStream, _input: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "`#[gw2_method]` has been replaced by `#[gw2_endpoint(path = \"...\")]` on an async fn",
    )
    .into_compile_error()
    .into()
}

// ── #[gw2_tagged_union] ──────────────────────────────────────────────────────

/// Annotate an enum whose `"type"` JSON field discriminates variants to generate
/// a `Deserialize` impl automatically. The enum must also `#[derive(Serialize)]`.
///
/// Rules:
/// - A variant named `Unknown` (with a `type_: String` field) is used as the
///   catch-all for unrecognised tags and is **not** matched explicitly.
/// - **Named-field variants** (`Foo { bar: T, baz: U }`) — each field is extracted
///   from the JSON object by key `"bar"` / `"baz"`.
/// - **Tuple variants** (`Foo(BarDetails)`) — the whole JSON object is deserialized
///   into `BarDetails` via `serde_json::from_value`.
/// - The JSON tag string is the variant name verbatim (e.g. `Coins` → `"Coins"`).
/// - A JSON object without `"type"` is an error, unless one **unit** variant is
///   marked `#[no_type]`: then that variant is returned (for documented blank objects).
#[proc_macro_attribute]
pub fn gw2_tagged_union(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_tagged_union(input)
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

fn expand_tagged_union(mut input: DeriveInput) -> syn::Result<TokenStream2> {
    // `#[no_type]` on one unit variant: the value for objects without a `"type"` field
    // (e.g. the blank achievement bits of recipe-unlock achievements). The marker is
    // not a real attribute, so strip it from the re-emitted enum.
    let mut no_type_variant: Option<Ident> = None;
    if let syn::Data::Enum(ref mut data) = input.data {
        for variant in &mut data.variants {
            let before = variant.attrs.len();
            variant.attrs.retain(|a| !a.path().is_ident("no_type"));
            if variant.attrs.len() != before {
                if !matches!(variant.fields, syn::Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "#[no_type] must be on a unit variant",
                    ));
                }
                if no_type_variant.replace(variant.ident.clone()).is_some() {
                    return Err(syn::Error::new_spanned(
                        &variant.ident,
                        "only one variant can be #[no_type]",
                    ));
                }
            }
        }
    }

    let syn::Data::Enum(ref data) = input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[gw2_tagged_union] can only be applied to enums",
        ));
    };

    let name = &input.ident;

    let mut arms = Vec::new();
    for variant in &data.variants {
        let vname = &variant.ident;
        if vname == "Unknown" || no_type_variant.as_ref() == Some(vname) {
            continue;
        }
        let tag = vname.to_string();

        let arm = match &variant.fields {
            syn::Fields::Unnamed(fields) => {
                if fields.unnamed.len() != 1 {
                    return Err(syn::Error::new_spanned(
                        vname,
                        "#[gw2_tagged_union] tuple variants must have exactly one field",
                    ));
                }
                let ty = &fields.unnamed[0].ty;
                let context = format!("{name}::{vname}");
                quote! {
                    #tag => ::gw2_api::ignored::from_value::<#ty>(#context, v)
                        .map(#name::#vname)
                        .map_err(::serde::de::Error::custom),
                }
            }
            syn::Fields::Named(fields) => {
                let context = format!("{name}::{vname}");
                let keys: Vec<String> = fields
                    .named
                    .iter()
                    .map(|f| f.ident.as_ref().unwrap().to_string())
                    .collect();
                let field_exprs: Vec<_> = fields
                    .named
                    .iter()
                    .map(|f| {
                        let fname = f.ident.as_ref().unwrap();
                        let key = fname.to_string();
                        let field_context = format!("{context}.{key}");
                        quote! {
                            #fname: ::gw2_api::ignored::from_value(#field_context, v[#key].take())
                                .map_err(::serde::de::Error::custom)?
                        }
                    })
                    .collect();
                quote! {
                    #tag => {
                        ::gw2_api::ignored::report_unknown_keys(#context, &v, &[#(#keys),*]);
                        ::std::result::Result::Ok(#name::#vname { #(#field_exprs,)* })
                    }
                }
            }
            syn::Fields::Unit => {
                return Err(syn::Error::new_spanned(
                    vname,
                    "#[gw2_tagged_union] does not support unit variants",
                ));
            }
        };
        arms.push(arm);
    }

    let missing_type = match &no_type_variant {
        Some(v) => quote! { return ::std::result::Result::Ok(#name::#v) },
        None => quote! { return ::std::result::Result::Err(::serde::de::Error::missing_field("type")) },
    };

    Ok(quote! {
        #input

        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::std::result::Result<Self, D::Error> {
                #[allow(unused_mut)]
                let mut v = ::serde_json::Value::deserialize(d)?;
                let type_str = match v.get("type").and_then(|t| t.as_str()) {
                    ::std::option::Option::Some(t) => t,
                    ::std::option::Option::None => #missing_type,
                };
                match type_str {
                    #(#arms)*
                    other => {
                        ::tracing::warn!(
                            enum_ = ::std::stringify!(#name),
                            type_ = other,
                            "unknown tagged-union variant from GW2 API",
                        );
                        ::std::result::Result::Ok(#name::Unknown { type_: other.to_owned() })
                    }
                }
            }
        }
    })
}

// ── #[gw2_enum] ──────────────────────────────────────────────────────────────

/// Annotate a **unit-variant-only** enum to:
/// - Append an `Unknown(String)` variant that captures any unrecognised API string.
/// - Generate `Serialize`, `Deserialize`, and `Display` impls.
/// - Round-trip correctly: `Unknown("Foo")` serializes as `"Foo"`, not `"Unknown"`.
///
/// ## Optional flag
///
/// `#[gw2_enum(lowercase)]` — wire values are the **lowercase** of the variant name
/// (e.g. `Account` → `"account"`).  Also generates a `Display` impl that uses the
/// same lowercase string, so you never need a manual `Display` or `rename_all`.
#[proc_macro_attribute]
pub fn gw2_enum(args: TokenStream, input: TokenStream) -> TokenStream {
    // Parse the optional `lowercase` flag from the attribute args.
    let lowercase = {
        let args2: TokenStream2 = args.into();
        // Accept either empty args or the single keyword `lowercase`.
        if args2.is_empty() {
            false
        } else {
            match syn::parse2::<syn::Path>(args2.clone()) {
                Ok(p) if p.is_ident("lowercase") => true,
                _ => {
                    return syn::Error::new_spanned(
                        args2,
                        "#[gw2_enum] only accepts an optional `lowercase` flag",
                    )
                    .into_compile_error()
                    .into();
                }
            }
        }
    };
    let input = parse_macro_input!(input as DeriveInput);
    expand_enum(input, lowercase)
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}

fn expand_enum(input: DeriveInput, lowercase: bool) -> syn::Result<TokenStream2> {
    let syn::Data::Enum(ref data) = input.data else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[gw2_enum] can only be applied to enums",
        ));
    };

    for v in &data.variants {
        if !matches!(v.fields, syn::Fields::Unit) {
            return Err(syn::Error::new_spanned(
                &v.ident,
                "#[gw2_enum] requires all variants to be unit variants (no fields)",
            ));
        }
    }

    let name = &input.ident;
    let attrs = &input.attrs;
    let vis = &input.vis;
    let variants: Vec<&Ident> = data.variants.iter().map(|v| &v.ident).collect();
    // For each variant, check for an explicit `#[serde(rename = "...")]` attribute.
    // If present, use that as the wire string; otherwise derive from the ident name.
    let variant_strs: Vec<String> = data
        .variants
        .iter()
        .map(|v| {
            // Look for #[serde(rename = "literal")] on this variant.
            let explicit_rename = v.attrs.iter().find_map(|attr| {
                if !attr.path().is_ident("serde") {
                    return None;
                }
                // Parse the serde attribute's token list for `rename = "..."`.
                let mut found = None;
                let _ = attr.parse_nested_meta(|meta| {
                    if meta.path.is_ident("rename") {
                        let value: LitStr = meta.value()?.parse()?;
                        found = Some(value.value());
                    }
                    Ok(())
                });
                found
            });
            explicit_rename.unwrap_or_else(|| {
                let s = v.ident.to_string();
                if lowercase { s.to_lowercase() } else { s }
            })
        })
        .collect();

    // Only generate Display when lowercase is set (PascalCase enums rarely need it
    // and callers can derive or impl it themselves).
    let display_impl = if lowercase {
        quote! {
            impl ::std::fmt::Display for #name {
                fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                    match self {
                        #(Self::#variants => f.write_str(#variant_strs),)*
                        Self::Unknown(v) => f.write_str(v),
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    Ok(quote! {
        #(#attrs)*
        #vis enum #name {
            #(#variants,)*
            /// Unrecognised variant returned by the API — the raw string is preserved.
            Unknown(::std::string::String),
        }

        impl ::serde::Serialize for #name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> ::std::result::Result<S::Ok, S::Error> {
                match self {
                    #(Self::#variants => s.serialize_str(#variant_strs),)*
                    Self::Unknown(v) => s.serialize_str(v),
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for #name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> ::std::result::Result<Self, D::Error> {
                let s = ::std::string::String::deserialize(d)?;
                match s.as_str() {
                    #(#variant_strs => ::std::result::Result::Ok(Self::#variants),)*
                    other => {
                        ::tracing::warn!(
                            enum_ = ::std::stringify!(#name),
                            value = other,
                            "unknown enum variant from GW2 API",
                        );
                        ::std::result::Result::Ok(Self::Unknown(other.to_owned()))
                    }
                }
            }
        }

        #display_impl
    })
}
