use proc_macro2::TokenStream;
use syn::parse::Nothing;

use crate::hygiene::Hygiene;
use crate::token_stream_with_error;

// Source:
//
// #[salsa::db]
// pub struct Database {
//    storage: salsa::Storage<Self>,
// }

pub(crate) fn db(
    args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let _nothing = syn::parse_macro_input!(args as Nothing);
    let hygiene = Hygiene::from1(&input);
    let item = parse_macro_input!(input as syn::Item);
    let db_macro = DbMacro { hygiene };
    match db_macro.try_db(item) {
        Ok(v) => crate::debug::dump_tokens("db", v).into(),
        Err(e) => token_stream_with_error(input, e),
    }
}

struct DbMacro {
    hygiene: Hygiene,
}

#[allow(non_snake_case)]
impl DbMacro {
    fn try_db(self, input: syn::Item) -> syn::Result<TokenStream> {
        match input {
            syn::Item::Struct(input) => {
                let has_storage_impl = self.has_storage_impl(&input)?;
                Ok(quote! {
                    #has_storage_impl
                    #input
                })
            }
            syn::Item::Trait(mut input) => {
                self.add_salsa_view_method(&mut input)?;
                Ok(quote! {
                    #input
                })
            }
            syn::Item::Impl(mut input) => {
                self.add_salsa_view_method_impl(&mut input)?;
                let register_view = self.register_view(&input);
                Ok(quote! {
                    #input
                    #register_view
                })
            }
            _ => Err(syn::Error::new_spanned(
                input,
                "`db` must be applied to a struct, trait, or impl",
            )),
        }
    }

    fn find_storage_field(&self, input: &syn::ItemStruct) -> syn::Result<syn::Ident> {
        let storage = "storage";
        for field in input.fields.iter() {
            if let Some(i) = &field.ident {
                if i == storage {
                    return Ok(i.clone());
                }
            } else {
                return Err(syn::Error::new_spanned(
                    field,
                    "database struct must be a braced struct (`{}`) with a field named `storage`",
                ));
            }
        }

        Err(syn::Error::new_spanned(
            &input.ident,
            "database struct must be a braced struct (`{}`) with a field named `storage`",
        ))
    }

    fn has_storage_impl(&self, input: &syn::ItemStruct) -> syn::Result<TokenStream> {
        let storage = self.find_storage_field(input)?;
        let db = &input.ident;
        let zalsa = self.hygiene.ident("zalsa");

        Ok(quote! {
            #[allow(clippy::all)]
            #[allow(dead_code)]
            const _: () = {
                use salsa::plumbing as #zalsa;

                unsafe impl #zalsa::HasStorage for #db {
                    #[inline(always)]
                    fn storage(&self) -> &#zalsa::Storage<Self> {
                        &self.#storage
                    }

                    #[inline(always)]
                    fn storage_mut(&mut self) -> &mut #zalsa::Storage<Self> {
                        &mut self.#storage
                    }
                }
            };
        })
    }

    fn add_salsa_view_method(&self, input: &mut syn::ItemTrait) -> syn::Result<()> {
        let trait_name = &input.ident;
        input.items.push(parse_quote! {
            #[doc(hidden)]
            fn zalsa_register_downcaster(&self) -> &salsa::plumbing::DatabaseDownCaster<dyn #trait_name>;
        });

        let comment = format!(" downcast `Self` to a [`dyn {trait_name}`]");
        input.items.push(parse_quote! {
            #[doc = #comment]
            #[doc(hidden)]
            fn downcast(&self) -> &dyn #trait_name where Self: Sized;
        });
        Ok(())
    }

    /// Registers the impl's view of its database type, so that the database holds the view from its
    /// creation. A generic impl has no single type to register for, so it registers nothing and
    /// its views are added when something first casts to them.
    fn register_view(&self, input: &syn::ItemImpl) -> TokenStream {
        let Some((TraitPath, _)) = &input.trait_ else {
            return TokenStream::new();
        };
        if !input.generics.params.is_empty() {
            return TokenStream::new();
        }
        let db = &input.self_ty;
        quote! {
            salsa::plumbing::register_view! {
                salsa::plumbing::ErasedView::erase::<#db>(|views| {
                    views.add::<#db, dyn #TraitPath>(unsafe {
                        ::std::mem::transmute(<#db as #TraitPath>::downcast as fn(_) -> _)
                    });
                })
            }
        }
    }

    fn add_salsa_view_method_impl(&self, input: &mut syn::ItemImpl) -> syn::Result<()> {
        let Some((TraitPath, _)) = &input.trait_ else {
            return Err(syn::Error::new_spanned(
                &input.self_ty,
                "impl must be on a trait",
            ));
        };

        input.items.push(parse_quote! {
            #[cold]
            #[inline(never)]
            #[doc(hidden)]
            fn zalsa_register_downcaster(&self) -> &salsa::plumbing::DatabaseDownCaster<dyn #TraitPath> {
                salsa::plumbing::views(self).add::<Self, dyn #TraitPath>(unsafe {
                    ::std::mem::transmute(<Self as #TraitPath>::downcast as fn(_) -> _)
                })
            }
        });
        input.items.push(parse_quote! {
            #[doc(hidden)]
            #[inline(always)]
            fn downcast(&self) -> &dyn #TraitPath where Self: Sized {
                self
            }
        });
        Ok(())
    }
}
