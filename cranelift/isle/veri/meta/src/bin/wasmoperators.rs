use wasmparser::for_each_operator;

macro_rules! print_operator {
    ($( @$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident)*) => {
        $(
            println!("{}\t{}",
                stringify!($proposal),
                stringify!($op),
            );
        )*
    }
}

pub fn main() {
    for_each_operator!(print_operator);
}
