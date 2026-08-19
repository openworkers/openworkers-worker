use proc_macro::TokenStream;

/// workers-rs uses this to paper over `!Send` JS futures. There are no JS
/// futures here, so the item passes through untouched.
pub fn expand_macro(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
