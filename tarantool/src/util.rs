pub trait IntoTupleOfClones<Tuple>: Clone {
    fn clones(self) -> Tuple;
}

macro_rules! impl_into_tuple_of_clones {
    // [@clones(self) T (...)] => [(... self,)]
    [@clones($self:ident) $h:ident ($($code:tt)*)] => { ($($code)* $self,) };
    // [@clones(self) T T ... T (...)] => [@clones(self) T ... T (... self.clone(),)]
    [@clones($self:ident) $h:ident $($t:ident)+ ($($code:tt)*)] => {
        impl_into_tuple_of_clones![
            @clones($self) $($t)+ ($($code)* $self.clone(),)
        ]
    };
    {$h:ident $($t:ident)*} => {
        impl<$h: Clone> IntoTupleOfClones<($h $(, $t)*,)> for $h {
            fn clones(self) -> ($h $(, $t)*,) {
                // [@clones(self) T T ... T ()]
                impl_into_tuple_of_clones![@clones(self) $h $($t)* ()]
            }
        }
        impl_into_tuple_of_clones!{$($t)*}
    };
    () => {};
}

impl_into_tuple_of_clones!{T T T T T T T T T T T}

