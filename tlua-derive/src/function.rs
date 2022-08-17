use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    parse_macro_input,
    FnArg, Lit,
    punctuated::Punctuated, Token,
};

pub fn function(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as syn::AttributeArgs);
    let ctx = Context::from_args(args);

    let input = parse_macro_input!(item as syn::Item);

    let syn::ItemFn { sig, block, .. } = match input {
        syn::Item::Fn(f) => f,
        _ => panic!("only `fn` items can be lua functions"),
    };

    let (ident, inputs, output, generics) = match sig {
        syn::Signature { asyncness: Some(_), .. } => {
            panic!("async lua functions are not supported yet")
        }
        syn::Signature { variadic: Some(_), .. } => {
            panic!("variadic lua functions are not supported yet")
        }
        syn::Signature { ident, inputs, output, generics, .. } => {
            (ident, inputs, output, generics)
        }
    };
    if !generics.params.is_empty() {
        panic!("Generics aren't supported")
    }

    let Inputs {
        inputs,
        input_types,
        input_pattern,
        input_idents,
        ..
    } = Inputs::parse(inputs);

    let Context {
        tlua,
        ..
    } = ctx;

    let inner_fn_name = syn::Ident::new("__tp_inner", ident.span());

    quote! {
        #[no_mangle]
        pub unsafe extern "C" fn #ident (__lua: #tlua::LuaState) -> ::std::os::raw::c_int {
            type __InputTypes = (#( #input_types, )*);
            let __guard = #tlua::PushGuard::from_input(__lua);
            let __args_count = __guard.size();
            let #input_pattern =
                match #tlua::LuaRead::lua_read_at_maybe_zero_position(__guard, -__args_count) {
                    ::std::result::Result::Ok(__args) => __args,
                    ::std::result::Result::Err(__guard) => {
                        #tlua::error!(&__guard, "{}",
                            #tlua::LuaError::wrong_type_passed::<__InputTypes, _>(&__guard, __args_count),
                        )
                    }
                };

            fn #inner_fn_name #generics (#inputs) #output {
                #block
            }

            let __res = __tp_inner(#(#input_idents),*);

            match #tlua::PushInto::push_into_lua(__res, __lua) {
                ::std::result::Result::Ok(__guard) => unsafe {
                    __guard.forget()
                }
                ::std::result::Result::Err(__err) => #tlua::error!(__lua, "{__err}"),
            }
        }
    }.into()

}

////////////////////////////////////////////////////////////////////////////////
// Context
////////////////////////////////////////////////////////////////////////////////

struct Context {
    tlua: TokenStream2,
}

impl Context {
    fn from_args(args: syn::AttributeArgs) -> Self {
        let mut tlua = quote! { ::tlua };

        use syn::NestedMeta::{Lit as NMLit, Meta as NMMeta};
        for arg in args {
            match arg {
                NMLit(lit) => {
                    eprintln!("unsuported attribute argument: {:?}", lit)
                }
                NMMeta(syn::Meta::NameValue(syn::MetaNameValue {
                    path,
                    lit,
                    ..
                })) if path.is_ident("tlua") => {
                    match &lit {
                        Lit::Str(s) => {
                            let tp: syn::Path = imp::parse_lit_str(s).unwrap();
                            tlua = quote! { #tp };
                        }
                        _ => panic!("tlua value must be a string literal"),
                    }
                }
                syn::NestedMeta::Meta(meta) => {
                    eprintln!("unsuported attribute argument: {:?}", meta)
                }
            }
        }

        Self {
            tlua,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Inputs
////////////////////////////////////////////////////////////////////////////////

struct Inputs {
    inputs: Punctuated<FnArg, Token![,]>,
    input_types: Vec<syn::Type>,
    input_pattern: TokenStream2,
    input_idents: Vec<syn::Pat>,
}

impl Inputs {
    fn parse(mut inputs: Punctuated<FnArg, Token![,]>) -> Self {
        let mut input_types = vec![];
        let mut input_idents = vec![];
        let mut actual_inputs = vec![];
        for i in &mut inputs {
            let syn::PatType { ref pat, ref ty, .. } = match i {
                FnArg::Receiver(_) => {
                    panic!("`self` receivers aren't supported in stored procedures")
                }
                FnArg::Typed(pat_ty) => pat_ty,
            };
            actual_inputs.push(pat.clone());
            input_types.push((**ty).clone());
            input_idents.push((**pat).clone());
        }

        let input_pattern = if inputs.is_empty() {
            quote!{ []: [(); 0] }
        } else {
            quote!{ ( #(#actual_inputs,)* ) }
        };

        Self {
            inputs,
            input_types,
            input_pattern,
            input_idents,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// imp
////////////////////////////////////////////////////////////////////////////////

mod imp {
    use proc_macro2::{Group, Span, TokenStream, TokenTree};
    use syn::parse::{self, Parse};

    // stolen from serde

    pub(crate) fn parse_lit_str<T>(s: &syn::LitStr) -> parse::Result<T>
    where
        T: Parse,
    {
        let tokens = spanned_tokens(s)?;
        syn::parse2(tokens)
    }

    fn spanned_tokens(s: &syn::LitStr) -> parse::Result<TokenStream> {
        let stream = syn::parse_str(&s.value())?;
        Ok(respan(stream, s.span()))
    }

    fn respan(stream: TokenStream, span: Span) -> TokenStream {
        stream
            .into_iter()
            .map(|token| respan_token(token, span))
            .collect()
    }

    fn respan_token(mut token: TokenTree, span: Span) -> TokenTree {
        if let TokenTree::Group(g) = &mut token {
            *g = Group::new(g.delimiter(), respan(g.stream(), span));
        }
        token.set_span(span);
        token
    }
}

