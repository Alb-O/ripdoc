//! Integration tests covering struct rendering scenarios.
mod utils;
use utils::*;

gen_tests! {
	tuple_struct, {
		idemp {
			complex: r#"
                pub struct ComplexTuple<'a, T, U>(&'a str, T, U, i32)
                where
                    T: Clone,
                    U: Default + 'a;
            "#
		}
		rt {
			with_private_fields: {
				input: r#"
                    pub struct PrivateFieldsTuple(pub i32, String, pub bool);
                "#,
				output: r#"
                    pub struct PrivateFieldsTuple(pub i32, _, pub bool);
                "#
			}
		}
		rt {
			generic_with_private_fields: {
				input: r#"
                    pub struct GenericPrivateTuple<T, U>(pub T, U);
                "#,
				output: r#"
                    pub struct GenericPrivateTuple<T, U>(pub T, _);
                "#
			}
		}
		rt {
			only_private_fields: {
				input: r#"
                    pub struct OnlyPrivateTuple(String, i32);
                "#,
				output: r#"
                    pub struct OnlyPrivateTuple(_, _);
                "#
			}
		}
		rt {
			private_struct: {
				input: r#"
                    struct PrivateTuple(i32, String);
                "#,
				output: r#"
				"#
			}
		}
	}
}

gen_tests! {
	unit_struct, {
		rt {
			private: {
				input: r#"
                    struct PrivateUnitStruct;
                "#,
				output: r#""#
			}
		}
	}
}

gen_tests! {
	plain_struct, {
		rt {
			with_private_fields: {
				input: r#"
                    pub struct PrivateFieldStruct {
                        pub field1: i32,
                        field2: String,
                    }
                "#,
				output: r#"
                    pub struct PrivateFieldStruct {
                        pub field1: i32,
                    }
                "#
			}
		}
		rt {
			generic_with_private_fields: {
				input: r#"
                    pub struct GenericPrivateFieldStruct<T, U> {
                        pub field1: T,
                        field2: U,
                    }
                "#,
				output: r#"
                    pub struct GenericPrivateFieldStruct<T, U> {
                        pub field1: T,
                    }
                "#
			}
		}
		rt {
			where_clause_with_private_fields: {
				input: r#"
                    pub struct WherePrivateFieldStruct<T, U>
                    where
                        T: Clone,
                        U: Default,
                    {
                        pub field1: T,
                        field2: U,
                    }
                "#,
				output: r#"
                    pub struct WherePrivateFieldStruct<T, U>
                    where
                        T: Clone,
                        U: Default,
                    {
                        pub field1: T,
                    }
                "#
			}
		}
		rt {
			only_private_fields: {
				input: r#"
                    pub struct OnlyPrivateFieldStruct {
                        field: String,
                    }
                "#,
				output: r#"
                    pub struct OnlyPrivateFieldStruct {}
                "#
			}
		}
	}
}
