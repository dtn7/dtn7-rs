mod cla;

extern crate proc_macro;
use proc_macro::TokenStream;

#[proc_macro]
pub fn init_cla_subsystem(_item: TokenStream) -> TokenStream {
    cla::init_cla_subsystem(_item)
}

#[proc_macro_attribute]
pub fn cla(metadata: TokenStream, input: TokenStream) -> TokenStream {
    cla::cla(metadata, input)
}
