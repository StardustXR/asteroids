use proc_macro::TokenStream;
use proc_macro2::TokenTree;
use quote::{ToTokens, format_ident, quote};
use syn::{Data, DeriveInput, Index, Member, parse_macro_input, parse_quote};

/// blends every `Mix` field unless it's marked `#[no_animate]`
///
/// also makes `{Name}Animation` with a curve setter per field, and implements `CustomElement`
/// by forwarding to the struct's `AnimatedElement` impl
///
/// tuple fields get setters named after their position, `_0`, `_1` and so on
#[proc_macro_derive(Animatable, attributes(no_animate))]
pub fn derive_animatable(input: TokenStream) -> TokenStream {
	let input = parse_macro_input!(input as DeriveInput);
	let name = &input.ident;
	let vis = &input.vis;
	let animation_ty = format_ident!("{name}Animation");
	let tracks_ty = format_ident!("{name}Tracks");

	let fields = match &input.data {
		Data::Struct(s) => &s.fields,
		_ => {
			return syn::Error::new_spanned(name, "Animatable can only be derived on structs")
				.to_compile_error()
				.into();
		}
	};

	let generics = &input.generics;
	let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
	let type_params: Vec<_> = generics.type_params().map(|p| &p.ident).collect();

	// whether a generic field is `Mix` is only known per use, so those never animate
	let animated: Vec<_> = fields
		.iter()
		.enumerate()
		.filter(|(_, f)| !f.attrs.iter().any(|a| a.path().is_ident("no_animate")))
		.filter(|(_, f)| !mentions(f.ty.to_token_stream(), &type_params))
		.map(|(i, f)| match &f.ident {
			Some(ident) => (Member::Named(ident.clone()), ident.clone(), f),
			None => (Member::Unnamed(Index::from(i)), format_ident!("_{i}"), f),
		})
		.collect();
	let idents: Vec<_> = animated.iter().map(|(_, part, _)| part).collect();
	let tys: Vec<_> = animated.iter().map(|(_, _, f)| &f.ty).collect();

	let a = quote!(::stardust_xr_asteroids);
	let p = quote!(#a::__private);

	let steps = animated.iter().map(|(member, part, f)| {
		let ty = &f.ty;
		quote! {
			(&#p::Probe::<#ty>::new()).step(
				&mut self.#member,
				&_old.#member,
				_animation.#part.as_ref(),
				&mut _tracks.#part,
				_now,
			);
		}
	});

	let mut element_generics = generics.clone();
	element_generics
		.params
		.insert(0, parse_quote!(__State: #a::ValidState));
	element_generics
		.make_where_clause()
		.predicates
		.push(parse_quote!(#name #ty_generics: #a::AnimatedElement<__State>));
	let (element_impl_generics, _, element_where_clause) = element_generics.split_for_impl();
	let animated_element = quote!(<#name #ty_generics as #a::AnimatedElement<__State>>);

	quote! {
		#vis struct #animation_ty #generics #where_clause {
			#(#idents: ::core::option::Option<#a::Curve>,)*
			_phantom: ::core::marker::PhantomData<fn() -> (#(#type_params,)*)>,
		}
		impl #impl_generics ::core::default::Default for #animation_ty #ty_generics #where_clause {
			fn default() -> Self {
				Self {
					#(#idents: ::core::option::Option::None,)*
					_phantom: ::core::marker::PhantomData,
				}
			}
		}
		impl #impl_generics #animation_ty #ty_generics #where_clause {
			#(
				// higher ranked so it's only checked where the setter is called
				pub fn #idents(mut self, curve: #a::Curve) -> Self
				where
					for<'__a> #tys: #a::Mix,
				{
					self.#idents = ::core::option::Option::Some(curve);
					self
				}
			)*
		}

		#[doc(hidden)]
		#vis struct #tracks_ty #generics #where_clause {
			#(#idents: ::core::option::Option<#p::Track<#tys>>,)*
			_phantom: ::core::marker::PhantomData<fn() -> (#(#type_params,)*)>,
		}
		impl #impl_generics ::core::default::Default for #tracks_ty #ty_generics #where_clause {
			fn default() -> Self {
				Self {
					#(#idents: ::core::option::Option::None,)*
					_phantom: ::core::marker::PhantomData,
				}
			}
		}

		impl #impl_generics #a::Animatable for #name #ty_generics #where_clause {
			type Animation = #animation_ty #ty_generics;
			type Tracks = #tracks_ty #ty_generics;
			fn blend(
				&mut self,
				_old: &Self,
				_animation: &Self::Animation,
				_tracks: &mut Self::Tracks,
				_now: ::core::option::Option<#p::Timestamp>,
			) {
				use #p::{ViaMix as _, ViaOther as _};
				#(#steps)*
			}
		}

		impl #element_impl_generics #a::CustomElement<__State> for #name #ty_generics #element_where_clause {
			type Inner = #p::AnimatedInner<
				#animated_element::Inner,
				<#name #ty_generics as #a::Animatable>::Tracks,
			>;
			type Error = #animated_element::Error;

			async fn create_inner(
				&self,
				context: &#a::Context,
				info: #a::CreateInnerInfo,
			) -> ::core::result::Result<Self::Inner, Self::Error> {
				::core::result::Result::Ok(#p::AnimatedInner {
					inner: #animated_element::create_inner(self, context, info).await?,
					tracks: ::core::default::Default::default(),
				})
			}
			fn diff(&self, old_self: &Self, context: &#a::Context, inner: &mut Self::Inner) {
				#animated_element::diff(self, old_self, context, &mut inner.inner)
			}
			fn frame(
				&self,
				context: &#a::Context,
				info: &#p::FrameInfo,
				state: &mut __State,
				inner: &mut Self::Inner,
			) {
				#animated_element::frame(self, context, info, state, &mut inner.inner)
			}
			fn blend_in_place(
				&mut self,
				old_self: &Self,
				context: &#a::Context,
				animation: ::core::option::Option<&(dyn ::core::any::Any + Send + Sync)>,
				inner: &mut Self::Inner,
			) {
				#p::blend(self, old_self, context, animation, &mut inner.tracks)
			}
		}
	}
	.into()
}

fn mentions(tokens: proc_macro2::TokenStream, idents: &[&syn::Ident]) -> bool {
	tokens.into_iter().any(|t| match t {
		TokenTree::Ident(i) => idents.contains(&&i),
		TokenTree::Group(g) => mentions(g.stream(), idents),
		_ => false,
	})
}
