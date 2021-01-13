use crate::ast::*;
use crate::code_gen::CodeGen;
use crate::error::Error;
use crate::hir::class_dict::ClassDict;
use crate::hir::hir_maker_context::*;
use crate::hir::method_dict::MethodDict;
use crate::hir::*;
use crate::names;
use crate::type_checking;

#[derive(Debug)]
pub struct HirMaker {
    /// List of classes found so far
    pub(super) class_dict: ClassDict,
    /// List of methods found so far
    pub(super) method_dict: MethodDict,
    /// List of constants found so far
    pub(super) constants: HashMap<ConstFullname, TermTy>,
    pub(super) const_inits: Vec<HirExpression>,
    /// List of string literals found so far
    pub(super) str_literals: Vec<String>,
    /// Stack of ctx
    pub(super) ctx_stack: Vec<HirMakerContext>,
    /// Counter to give unique name for lambdas
    pub(super) lambda_ct: usize,
}

pub fn make_hir(ast: ast::Program, corelib: Corelib) -> Result<Hir, Error> {
    let class_dict = class_dict::create(&ast, corelib.sk_classes)?;
    let mut hir = convert_program(class_dict, ast)?;

    // While corelib classes are included in `class_dict`,
    // corelib methods are not. Here we need to add them manually
    hir.add_methods(corelib.sk_methods);

    Ok(hir)
}

fn convert_program(class_dict: ClassDict, prog: ast::Program) -> Result<Hir, Error> {
    let mut hir_maker = HirMaker::new(class_dict);
    let (main_exprs, main_lvars) = hir_maker.convert_toplevel_items(&prog.toplevel_items)?;
    Ok(hir_maker.extract_hir(main_exprs, main_lvars))
}

impl HirMaker {
    fn new(class_dict: ClassDict) -> HirMaker {
        HirMaker {
            class_dict,
            method_dict: MethodDict::new(),
            constants: HashMap::new(),
            const_inits: vec![],
            str_literals: vec![],
            ctx_stack: vec![],
            lambda_ct: 0,
        }
    }

    /// Destructively convert self to Hir
    fn extract_hir(&mut self, main_exprs: HirExpressions, main_lvars: HirLVars) -> Hir {
        // Extract data from self
        let sk_classes = std::mem::replace(&mut self.class_dict.sk_classes, HashMap::new());
        let sk_methods = std::mem::take(&mut self.method_dict.sk_methods);
        let mut constants = HashMap::new();
        std::mem::swap(&mut constants, &mut self.constants);
        let mut str_literals = vec![];
        std::mem::swap(&mut str_literals, &mut self.str_literals);
        let mut const_inits = vec![];
        std::mem::swap(&mut const_inits, &mut self.const_inits);

        // Register void
        constants.insert(const_fullname("::Void"), ty::raw("Void"));

        Hir {
            sk_classes,
            sk_methods,
            constants,
            str_literals,
            const_inits,
            main_exprs,
            main_lvars,
        }
    }

    fn convert_toplevel_items(
        &mut self,
        items: &[ast::TopLevelItem],
    ) -> Result<(HirExpressions, HirLVars), Error> {
        let mut main_exprs = vec![];
        // Contains local vars defined at toplevel
        self.push_ctx(HirMakerContext::toplevel());
        for item in items {
            match item {
                ast::TopLevelItem::Def(def) => {
                    self.process_toplevel_def(&def)?;
                }
                ast::TopLevelItem::Expr(expr) => {
                    main_exprs.push(self.convert_expr(&expr)?);
                }
            }
        }
        let mut ctx = self.pop_ctx();
        Ok((HirExpressions::new(main_exprs), ctx.extract_lvars()))
    }

    fn process_toplevel_def(&mut self, def: &ast::Definition) -> Result<(), Error> {
        match def {
            // Extract instance/class methods
            ast::Definition::ClassDefinition { name, defs, typarams, .. } => {
                let full = name.add_namespace("");
                self.process_defs_in_class(&full, typarams.clone(), defs)?;
            }
            ast::Definition::ConstDefinition { name, expr } => {
                self.register_const(name, expr)?;
            }
            _ => panic!("should be checked in hir::class_dict"),
        }
        Ok(())
    }

    /// Process each method def and const def
    fn process_defs_in_class(
        &mut self,
        fullname: &ClassFullname,
        typarams: Vec<String>,
        defs: &[ast::Definition],
    ) -> Result<(), Error> {
        let meta_name = fullname.meta_name();
        let ctx = HirMakerContext::class_ctx(&fullname, typarams, self.next_ctx_depth());
        self.push_ctx(ctx);

        // Register constants before processing #initialize
        self._process_const_defs_in_class(defs, fullname)?;

        // Add `#initialize`
        let mut own_ivars = HashMap::default();
        if let Some(ast::Definition::InstanceMethodDefinition {
            sig, body_exprs, ..
        }) = defs.iter().find(|d| d.is_initializer())
        {
            let (sk_method, found_ivars) =
                self.create_initialize(&fullname, &sig.name, &body_exprs)?;
            self.method_dict.add_method(&fullname, sk_method);
            own_ivars = found_ivars;
        }
        self.define_ivars(fullname, own_ivars, defs)?;

        self.method_dict
            .add_method(&meta_name, self.create_new(&fullname)?);

        for def in defs {
            match def {
                ast::Definition::InstanceMethodDefinition {
                    sig, body_exprs, ..
                } => {
                    if def.is_initializer() {
                        // Already processed above
                    } else {
                        let method =
                            self.convert_method_def(&fullname, &sig.name, &body_exprs)?;
                        self.method_dict.add_method(&fullname, method);
                    }
                }
                ast::Definition::ClassMethodDefinition {
                    sig, body_exprs, ..
                } => {
                    let method =
                        self.convert_method_def(&meta_name, &sig.name, &body_exprs)?;
                    self.method_dict.add_method(&meta_name, method);
                }
                ast::Definition::ConstDefinition { .. } => {
                    // Already processed above
                    ()
                }
                ast::Definition::ClassDefinition { name, typarams, defs, .. } => {
                    let full = name.add_namespace(&fullname.0);
                    self.process_defs_in_class(&full, typarams.clone(), defs)?;
                }
            }
        }
        self.pop_ctx();
        Ok(())
    }

    /// Register constants defined in a class
    fn _process_const_defs_in_class(
        &mut self,
        defs: &[ast::Definition],
        fullname: &ClassFullname,
    ) -> Result<(), Error> {
        for def in defs {
            if let ast::Definition::ConstDefinition { name, expr } = def {
                let full = name.add_namespace(&fullname.0);
                let hir_expr = self.convert_expr(expr)?;
                self.register_const_full(full, hir_expr);
            }
        }
        Ok(())
    }

    /// Create the `initialize` method
    /// Also, define ivars
    fn create_initialize(
        &mut self,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
    ) -> Result<(SkMethod, SkIVars), Error> {
        let super_ivars = self
            .class_dict
            .get_superclass(class_fullname)
            .map(|super_cls| super_cls.ivars.clone());
        self.convert_method_def_(class_fullname, name, body_exprs, true, super_ivars)
    }

    /// Define ivars of a class
    /// Also, define accessors
    fn define_ivars(
        &mut self,
        clsname: &ClassFullname,
        own_ivars: SkIVars,
        defs: &[ast::Definition],
    ) -> Result<(), Error> {
        self.class_dict.define_ivars(clsname, own_ivars.clone())?;
        self.define_accessors(clsname, own_ivars, defs);
        Ok(())
    }

    /// Create .new
    fn create_new(&self, class_fullname: &ClassFullname) -> Result<SkMethod, Error> {
        let class_fullname = class_fullname.clone();
        let (initialize_name, init_cls_name) =
            self._find_initialize(&class_fullname.instance_ty())?;
        let need_bitcast = init_cls_name != class_fullname;
        let (signature, _) = self
            .class_dict
            .lookup_method(&class_fullname.class_ty(), &method_firstname("new"), &[])?;
        let arity = signature.params.len();

        let new_body = move |code_gen: &CodeGen, function: &inkwell::values::FunctionValue| {
            // Allocate memory
            let obj = code_gen.allocate_sk_obj(&class_fullname, "addr");

            // Call initialize
            let initialize = code_gen
                .module
                .get_function(&initialize_name.full_name)
                .unwrap_or_else(|| panic!("[BUG] function `{}' not found", &initialize_name));
            let mut addr = obj;
            if need_bitcast {
                let ances_type = code_gen
                    .llvm_struct_types
                    .get(&init_cls_name)
                    .expect("ances_type not found")
                    .ptr_type(inkwell::AddressSpace::Generic);
                addr = code_gen
                    .builder
                    .build_bitcast(addr, ances_type, "obj_as_super");
            }
            let args = (0..=arity)
                .map(|i| {
                    if i == 0 {
                        addr
                    } else {
                        function.get_params()[i]
                    }
                })
                .collect::<Vec<_>>();
            code_gen.builder.build_call(initialize, &args, "");

            code_gen.builder.build_return(Some(&obj));
            Ok(())
        };

        Ok(SkMethod {
            signature,
            body: SkMethodBody::RustClosureMethodBody {
                boxed_gen: Box::new(new_body),
            },
            lvars: vec![],
        })
    }

    /// Find actual `initialize` func to call from `.new`
    fn _find_initialize(&self, class: &TermTy) -> Result<(MethodFullname, ClassFullname), Error> {
        let (_, found_cls) = self
            .class_dict
            .lookup_method(&class, &method_firstname("initialize"), &[])?;
        Ok((names::method_fullname(&found_cls, "initialize"), found_cls))
    }

    /// Resolve and register a constant
    pub(super) fn register_const(
        &mut self,
        name: &ConstFirstname,
        expr: &AstExpression,
    ) -> Result<ConstFullname, Error> {
        let ctx = self.ctx();
        // TODO: resolve name using ctx
        let fullname = const_fullname(&format!("{}::{}", ctx.namespace.0, &name.0));
        let hir_expr = self.convert_expr(expr)?;
        Ok(self.register_const_full(fullname, hir_expr))
    }

    /// Register a constant
    pub(super) fn register_const_full(
        &mut self,
        fullname: ConstFullname,
        hir_expr: HirExpression,
    ) -> ConstFullname {
        self.constants.insert(fullname.clone(), hir_expr.ty.clone());
        let op = Hir::const_assign(fullname.clone(), hir_expr);
        self.const_inits.push(op);
        fullname
    }

    fn convert_method_def(
        &mut self,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
    ) -> Result<SkMethod, Error> {
        let (sk_method, _ivars) =
            self.convert_method_def_(class_fullname, name, body_exprs, false, None)?;
        Ok(sk_method)
    }

    /// Create a SkMethod and return it with ctx.iivars
    fn convert_method_def_(
        &mut self,
        class_fullname: &ClassFullname,
        name: &MethodFirstname,
        body_exprs: &[AstExpression],
        is_initializer: bool,
        super_ivars: Option<SkIVars>,
    ) -> Result<(SkMethod, HashMap<String, SkIVar>), Error> {
        // MethodSignature is built beforehand by class_dict::new
        let err = format!(
            "[BUG] signature not found ({}/{}/{:?})",
            class_fullname, name, self.class_dict
        );
        let signature = self
            .class_dict
            .find_method(class_fullname, name)
            .expect(&err)
            .clone();

        self.push_ctx(HirMakerContext::method_ctx(
            self.ctx(),
            &signature,
            is_initializer,
            super_ivars.unwrap_or_else(HashMap::new),
        ));
        let body_exprs = self.convert_exprs(body_exprs)?;
        let mut method_ctx = self.pop_ctx();
        let lvars = method_ctx.extract_lvars();
        let iivars = method_ctx.iivars;
        type_checking::check_return_value(&self.class_dict, &signature, &body_exprs.ty)?;

        let body = SkMethodBody::ShiikaMethodBody { exprs: body_exprs };
        Ok((
            SkMethod {
                signature,
                body,
                lvars,
            },
            iivars,
        ))
    }
}
