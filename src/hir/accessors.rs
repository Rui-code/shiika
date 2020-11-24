use crate::code_gen::CodeGen;
use crate::hir::hir_maker::HirMaker;
use crate::hir::*;

impl HirMaker {
    /// Define getters and setters (unless there is a method of the same name)
    pub(super) fn define_accessors(
        &mut self,
        clsname: &ClassFullname,
        ivars: SkIVars,
        defs: &[ast::Definition],
    ) {
        let method_names = defs
            .iter()
            .filter_map(|def| {
                if let ast::Definition::InstanceMethodDefinition { sig, .. } = def {
                    Some(&sig.name.0)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        for (name, ivar) in ivars {
            if !method_names.iter().any(|x| ***x == name) {
                let getter = create_getter(&clsname, &ivar);
                let sig = getter.signature.clone();
                self.method_dict.add_method(&clsname, getter);
                self.class_dict.add_method(&clsname, sig);
            }

            let setter_name = format!("{}=", name);
            if !method_names.iter().any(|x| ***x == setter_name) {
                let setter = create_setter(&clsname, &ivar);
                let sig = setter.signature.clone();
                self.method_dict.add_method(&clsname, setter);
                self.class_dict.add_method(&clsname, sig);
            }
        }
    }
}

fn create_getter(clsname: &ClassFullname, ivar: &SkIVar) -> SkMethod {
    let sig = MethodSignature {
        fullname: method_fullname(clsname, &ivar.name),
        ret_ty: ivar.ty.clone(),
        params: vec![],
    };
    let name = ivar.name.clone(); // Clone to embed into the closure
    let idx = ivar.idx;
    let getter_body = move |code_gen: &CodeGen, function: &inkwell::values::FunctionValue| {
        let this = function.get_params()[0];
        let val = code_gen.build_ivar_load(this, idx, &name);
        code_gen.builder.build_return(Some(&val));
        Ok(())
    };

    SkMethod {
        signature: sig,
        body: SkMethodBody::RustClosureMethodBody {
            boxed_gen: Box::new(getter_body),
        },
        lvars: vec![],
    }
}

fn create_setter(clsname: &ClassFullname, ivar: &SkIVar) -> SkMethod {
    let name = format!("{}=", ivar.name);
    let sig = MethodSignature {
        fullname: method_fullname(clsname, &name),
        ret_ty: ivar.ty.clone(),
        params: vec![MethodParam {
            name: ivar.name.clone(),
            ty: ivar.ty.clone(),
        }],
    };
    let idx = ivar.idx;
    let getter_body = move |code_gen: &CodeGen, function: &inkwell::values::FunctionValue| {
        let this = function.get_params()[0];
        let val = function.get_params()[1];
        code_gen.build_ivar_store(&this, idx, val, &name);
        code_gen.builder.build_return(Some(&val));
        Ok(())
    };

    SkMethod {
        signature: sig,
        body: SkMethodBody::RustClosureMethodBody {
            boxed_gen: Box::new(getter_body),
        },
        lvars: vec![],
    }
}
