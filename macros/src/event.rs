use proc_macro::TokenStream;
use proc_macro2::Span;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_crate::crate_name;
use proc_macro_crate::FoundCrate;
use quote::quote;
use syn::parse_macro_input;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use syn::Ident;
use syn::ItemFn;

enum HandlerType {
    Fetch,
    Scheduled,
}

/// The path the SDK is reachable under from the caller's crate. A workers-rs
/// port renames the package to `worker`, so the name cannot be hardcoded.
fn sdk_path() -> TokenStream2 {
    match crate_name("openworkers-worker") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(::worker),
    }
}

fn validate(input_fn: &ItemFn, handler: &str) -> Result<(), TokenStream> {
    let sig = &input_fn.sig;

    if sig.asyncness.is_none() {
        return Err(err(
            sig.ident.span(),
            format!("the `{handler}` handler must be an async function"),
        ));
    }

    if sig.inputs.len() != 3 {
        return Err(err(
            sig.ident.span(),
            format!(
                "the `{handler}` handler should be a function with 3 arguments but found {}",
                sig.inputs.len()
            ),
        ));
    }

    Ok(())
}

fn err(span: Span, message: String) -> TokenStream {
    syn::Error::new(span, message).to_compile_error().into()
}

pub fn expand_macro(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attrs: Punctuated<Ident, Comma> =
        parse_macro_input!(attr with Punctuated::parse_terminated);

    let mut handler_type = None;
    let mut respond_with_errors = false;

    for attr in attrs {
        match attr.to_string().as_str() {
            "respond_with_errors" => respond_with_errors = true,
            "fetch" => handler_type = Some(HandlerType::Fetch),
            "scheduled" => handler_type = Some(HandlerType::Scheduled),
            other => {
                return err(
                    attr.span(),
                    format!(
                        "`{other}` events are not supported by openworkers-worker; \
                         use `fetch` or `scheduled`"
                    ),
                )
            }
        }
    }

    let Some(handler_type) = handler_type else {
        return err(
            Span::call_site(),
            "must have either the 'fetch' or 'scheduled' attribute, e.g. #[event(fetch)]"
                .to_string(),
        );
    };

    let input_fn = parse_macro_input!(item as ItemFn);
    let fn_ident = input_fn.sig.ident.clone();
    let krate = sdk_path();

    match handler_type {
        HandlerType::Fetch => {
            if let Err(e) = validate(&input_fn, "fetch") {
                return e;
            }

            let component = Ident::new(&format!("__OpenWorkersFetch_{fn_ident}"), fn_ident.span());

            // The SDK picks the exported world by feature, which the macro
            // cannot see from here
            quote! {
                #input_fn

                #[doc(hidden)]
                #[allow(non_camel_case_types)]
                struct #component;

                #krate::__emit_fetch_export!(#component, #fn_ident, #respond_with_errors);
            }
            .into()
        }
        HandlerType::Scheduled => {
            if let Err(e) = validate(&input_fn, "scheduled") {
                return e;
            }

            let component = Ident::new(
                &format!("__OpenWorkersScheduled_{fn_ident}"),
                fn_ident.span(),
            );

            quote! {
                #input_fn

                #[doc(hidden)]
                #[allow(non_camel_case_types)]
                struct #component;

                impl #krate::wit_scheduled::exports::openworkers::worker::scheduled::Guest
                    for #component
                {
                    fn handle_scheduled(scheduled_time: u64) {
                        #krate::__private::serve_scheduled(scheduled_time, #fn_ident)
                    }
                }

                #krate::wit_scheduled::__export_worker_scheduled!(#component with_types_in #krate::wit_scheduled);
            }
            .into()
        }
    }
}
