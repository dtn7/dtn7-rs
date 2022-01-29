extern crate proc_macro;
use proc_macro::TokenStream;
use quote::{format_ident, quote};

use lazy_static::lazy_static;
use std::sync::Mutex;
use syn::parse_macro_input;

lazy_static! {
    static ref CLAS: Mutex<Vec<String>> = Mutex::new(vec![]);
}
pub fn init_cla_subsystem(_item: TokenStream) -> TokenStream {
    let cla_subsystem = CLAS
        .lock()
        .unwrap()
        .iter()
        .map(|c| format_ident!("{}", c))
        .collect::<Vec<_>>();

    let cla_list = quote! {
        pub fn convergence_layer_agents() -> Vec<&'static str> {
            vec![#(#cla_subsystem::human_name()),*]
        }
    };

    let local_help = quote! {
        pub fn local_help() -> Vec<String> {
            vec![#(format!("{}:{}",#cla_subsystem::human_name(),#cla_subsystem::local_help_str())),*]
        }
    };

    let global_help = quote! {
        pub fn global_help() -> Vec<String> {
            vec![#(format!("{}:{}",#cla_subsystem::human_name(),#cla_subsystem::global_help_str())),*]
        }
    };

    let cla_enum = quote! {
        #[enum_dispatch]
        #[derive(Debug, Display)]
        pub enum CLAEnum {
            #(#cla_subsystem),*
        }
    };

    let clas_available = quote! {
        #[derive(Debug, Display, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
        pub enum CLAsAvailable {
            #(#cla_subsystem),*
        }
        impl From<CLAsAvailable> for &'static str {
            fn from(v: CLAsAvailable) -> Self {
                match v {
                    #(CLAsAvailable::#cla_subsystem => #cla_subsystem::human_name()),*
                }
            }
        }
        impl FromStr for CLAsAvailable {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                #(if s == #cla_subsystem::human_name() {
                    return Ok(Self::#cla_subsystem)
                })*
                Err(format!("{} is not a valid CLAsAvailable", s))
            }
        }
        impl From<&str> for CLAsAvailable {
            fn from(s: &str) -> Self {
                Self::from_str(s).unwrap()
            }
        }
    };

    let cla_new = quote! {
        pub fn new(cla: &CLAsAvailable, local_settings: Option<&HashMap<String, String>>) -> CLAEnum {
            #(if *cla == CLAsAvailable::#cla_subsystem {
                return #cla_subsystem::new(local_settings).into();
            })*
            panic!("Unknown convergence layer agent agent {}", cla);
        }
    };

    let all = quote! {
        #cla_list
        #local_help
        #global_help
        #cla_enum
        #clas_available
        #cla_new
    };

    //println!("quote: {}", all);
    all.into()
}

pub fn cla(metadata: TokenStream, input: TokenStream) -> TokenStream {
    //println!("attr: \"{}\"", metadata.to_string());
    let human_name = metadata.to_string();
    let input = parse_macro_input!(input as syn::ItemStruct);
    let struct_name = input.ident.to_string();
    CLAS.lock().unwrap().push(struct_name.clone());

    let struct_name2 = syn::Ident::new(&struct_name, input.ident.span());
    let res = quote! {
        #input

        impl #struct_name2 {
            fn my_name(&self) -> &'static str {
                #human_name
            }
            pub fn human_name() -> &'static str {
                #human_name
            }
        }

    };

    res.into()
}
