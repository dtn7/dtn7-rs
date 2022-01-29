/*
use dtn7_codegen::*;

pub trait ConvergenceLayerAgent {
    fn name(&self) -> &'static str;
}
pub trait HelpStr {
    fn local_help_str() -> &'static str {
        "<>"
    }
    fn global_help_str() -> &'static str {
        "<>"
    }
}

#[cla(mtcp)]
#[derive(Debug)]
struct A {}

impl HelpStr for A {
    fn local_help_str() -> &'static str {
        "a"
    }
    fn global_help_str() -> &'static str {
        "global: a"
    }
}

#[cla(http)]
#[derive(Debug)]
struct B {}

impl HelpStr for B {}

//init_cla_subsystem!();

//#[test]
fn test_cla() {
    let a = A {};
    let b = B {};
    println!("{:?}", a.name());
    println!("{:?}", b);

    println!("{:?}", local_help());
    println!("{:?}", global_help());

    println!("{:?}", convergence_layer_agents());

    //println!("all clas: {}", all_clas());
    //assert_eq!(a.get_name(), "A");
    //assert_eq!(b.get_name(), "B");
}
*/
