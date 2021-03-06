pub mod vtable;
use crate::hir::Hir;
pub use crate::mir::vtable::VTables;

#[derive(Debug)]
pub struct Mir {
    pub hir: Hir,
    pub vtables: VTables,
}

pub fn build(hir: Hir) -> Mir {
    let vtables = VTables::build(&hir.sk_classes);
    Mir { hir, vtables }
}
