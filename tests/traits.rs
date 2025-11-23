//! Integration tests ensuring trait rendering stays stable.
mod utils;
use utils::*;

gen_tests! {
	traits, {
		idemp {
			with_associated_type_bounds: r#"
                pub trait BoundedAssocType {
                    type Item: Clone + 'static;
                    fn get_item(&self) -> Self::Item;
                }
            "#
		}
		idemp {
			unsafe_trait: r#"
                pub unsafe trait UnsafeTrait {
                    unsafe fn unsafe_method(&self);
                }
            "#
		}
		rt {
			private_items: {
				input: r#"
                    pub trait TraitWithPrivateItems {
                        fn public_method(&self);
                        #[doc(hidden)]
                        fn private_method(&self);
                        type PublicType;
                        #[doc(hidden)]
                        type PrivateType;
                    }
                "#,
				output: r#"
                    pub trait TraitWithPrivateItems {
                        fn public_method(&self);
                        type PublicType;
                    }
                "#
			}
		}
		rt {
			private_trait: {
				input: r#"
                    trait PrivateTrait {
                        fn method(&self);
                    }
                "#,
				output: r#"
				"#
			}
		}
	}
}
