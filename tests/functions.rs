//! Integration tests validating function signature rendering.
mod utils;
use ripdoc::RenderFormat;
use ripdoc::core_api::Renderer;
use utils::*;

gen_tests! {
	functions, {
		idemp {
			complex: r#"
                pub async unsafe fn complex_function<'a, T, U>(x: &'a T, y: U) -> Result<T, U>
                where
                    T: Clone + Send + 'a,
                    U: std::fmt::Debug,
                {
                }
            "#
		}
		idemp {
			hrtb: r#"
                pub fn hrtb_function<F>(f: F)
                where
                    for<'a> F: Fn(&'a str) -> bool,
                {
                }
            "#
		}
		idemp {
			dyn_trait_parens: r#"
                pub fn myfn() -> &'static (dyn std::any::Any + 'static) { }
            "#
		}
		idemp {
			dyn_trait_with_associated_type: r#"
                pub trait Iterator {
                    type Item;
                    fn next(&mut self) -> Option<Self::Item>;
                }
                pub fn function_with_dyn_iterator(iter: &mut dyn Iterator<Item = i32>) {}
            "#
		}
		idemp {
			impl_trait_with_multiple_bounds: r#"
                pub fn request_value<'a, T>(err: &'a (impl std::error::Error + ?Sized)) -> Option<T>
                where
                    T: 'static
                {
                }
            "#
		}
		rt {
			private_function: {
				input: r#"
                    fn private_function() {}
                "#,
				output: r#"
				"#
			}
		}
		rt {
			with_doc_comments: {
				input: r#"
                    /// This is a documented function.
                    /// It has multiple lines of documentation.
                    pub fn documented_function() {}
                "#,
				output: r#"
                    /// This is a documented function.
                    /// It has multiple lines of documentation.
                    pub fn documented_function() {}
                "#
			}
		}
		rt {
		   with_attributes: {
				input: r#"
                    #[inline]
                    #[cold]
                    pub fn function_with_attributes() {}
                "#,
				output: r#"
                    pub fn function_with_attributes() {}
                "#
			}
		}
		rt_custom {
			render_private: {
				renderer: Renderer::default().with_format(RenderFormat::Rust).with_private_items(true),
				input: r#"
                    fn private_function() {}
                    pub fn public_function() {}
                "#,
				output: r#"
                    fn private_function() {}
                    pub fn public_function() {}
                "#
			}
		}
	}

}
