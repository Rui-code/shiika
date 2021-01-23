use crate::hir::hir_maker::HirMaker;
use crate::hir::*;
use crate::names::*;
use crate::ty;
use crate::ty::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct HirMakerContext {
    /// Type of this ctx
    pub kind: CtxKind,
    /// Where this ctx is in the ctx_stack
    pub depth: usize,
    /// The type of current `self`
    pub self_ty: TermTy,
    /// Current namespace
    /// `""` for toplevel
    pub namespace: ClassFullname,
    /// Current local variables
    pub lvars: HashMap<String, CtxLVar>,
}

#[derive(Debug, PartialEq)]
pub enum CtxKind {
    Toplevel,
    Class,
    Method,
    Initializer,
    Lambda,
}

#[derive(Debug)]
pub struct HirMakerContext_ {
    pub toplevel: ToplevelCtx,
    pub classes: Vec<ClassCtx>,
    pub method: Option<MethodCtx>,
    pub lambdas: Vec<LambdaCtx>,
}

impl HirMakerContext_ {
    /// Create initial ctx
    pub fn new() -> HirMakerContext_ {
        HirMakerContext_ {
            toplevel: ToplevelCtx {},
            classes: vec![],
            method: None,
            lambdas: vec![],
        }
    }

    /// Return method/lambda argument of given name, if any
    pub fn find_fn_arg(&self, name: &str) -> Option<(usize, &MethodParam)> {
        let params = if let Some(lambda_ctx) = self.lambdas.last() {
            &lambda_ctx.params
        } else {
            &self.method.as_ref().unwrap().signature.params
        };
        signature::find_param(&params, name)
    }

    /// Push a LambdaCapture to captures
    pub fn push_lambda_capture(&mut self, cap: LambdaCapture) -> usize {
        let lambda_ctx = self.lambdas.last_mut().expect("not in lambda");
        lambda_ctx.captures.push(cap);
        lambda_ctx.captures.len() - 1
    }
}

#[derive(Debug)]
pub struct ToplevelCtx {}

#[derive(Debug)]
pub struct ClassCtx {}

#[derive(Debug)]
pub struct MethodCtx {
    /// Signature of the current method
    signature: MethodSignature,
    /// List of instance variables in an initializer found so far.
    /// Empty if the method is not `#initialize`
    pub iivars: SkIVars,
    /// List of inherited ivars
    /// Empty if the method is not `#initialize`
    pub super_ivars: SkIVars, // TODO: this can be just &'a SkIVars
}

impl MethodCtx {
    pub fn new(signature: MethodSignature, super_ivars: Option<SkIVars>) -> MethodCtx {
        MethodCtx {
            signature,
            iivars: Default::default(),
            super_ivars: super_ivars.unwrap_or(Default::default()),
        }
    }
}

#[derive(Debug)]
pub struct LambdaCtx {
    /// Parameters of the current lambda
    pub params: Vec<MethodParam>,
    /// List of free variables captured in this context
    pub captures: Vec<LambdaCapture>,
}

impl LambdaCtx {
    pub fn new(params: Vec<MethodParam>) -> LambdaCtx {
        LambdaCtx {
            params,
            captures: Default::default(),
        }
    }
}

/// A local variable
#[derive(Debug)]
pub struct CtxLVar {
    pub name: String,
    pub ty: TermTy,
    pub readonly: bool,
}

#[derive(Debug)]
pub struct LambdaCapture {
    pub ctx_depth: usize,
    pub ty: TermTy,
    pub detail: LambdaCaptureDetail,
}

#[derive(Debug)]
pub enum LambdaCaptureDetail {
    CapLVar { name: String },
    CapFnArg { idx: usize },
}

impl HirMakerContext {
    /// Create a ctx for toplevel
    pub fn toplevel() -> HirMakerContext {
        // REVIEW: not sure this 'static is the right way
        HirMakerContext {
            kind: CtxKind::Toplevel,
            depth: 0,
            self_ty: ty::raw("Object"),
            namespace: ClassFullname("".to_string()),
            lvars: HashMap::new(),
        }
    }

    /// Create a class context
    pub fn class_ctx(fullname: &ClassFullname, depth: usize) -> HirMakerContext {
        HirMakerContext {
            kind: CtxKind::Class,
            depth,
            self_ty: ty::raw("Object"),
            namespace: fullname.clone(),
            lvars: HashMap::new(),
        }
    }

    /// Create a method context
    pub fn method_ctx(class_ctx: &HirMakerContext) -> HirMakerContext {
        HirMakerContext {
            kind: CtxKind::Method,
            depth: class_ctx.depth + 1,
            self_ty: ty::raw(&class_ctx.namespace.0),
            namespace: class_ctx.namespace.clone(),
            lvars: HashMap::new(),
        }
    }

    /// Create a initializer context
    pub fn initializer_ctx(class_ctx: &HirMakerContext) -> HirMakerContext {
        HirMakerContext {
            kind: CtxKind::Initializer,
            depth: class_ctx.depth + 1,
            self_ty: ty::raw(&class_ctx.namespace.0),
            namespace: class_ctx.namespace.clone(),
            lvars: HashMap::new(),
        }
    }

    /// Create a ctx for lambda
    pub fn lambda_ctx(method_ctx: &HirMakerContext) -> HirMakerContext {
        HirMakerContext {
            kind: CtxKind::Lambda,
            depth: method_ctx.depth + 1,
            self_ty: method_ctx.self_ty.clone(),
            namespace: method_ctx.namespace.clone(),
            lvars: HashMap::new(),
        }
    }

    /// Return local variable of given name, if any
    pub fn find_lvar(&self, name: &str) -> Option<&CtxLVar> {
        self.lvars.get(name)
    }

    /// Destructively extract list of local variables
    pub fn extract_lvars(&mut self) -> HirLVars {
        std::mem::take(&mut self.lvars)
            .into_iter()
            .map(|(name, ctx_lvar)| (name, ctx_lvar.ty))
            .collect::<Vec<_>>()
    }
}

impl HirMaker {
    pub(super) fn ctx(&self) -> &HirMakerContext {
        self.ctx_stack.last().unwrap()
    }

    pub(super) fn ctx_mut(&mut self) -> &mut HirMakerContext {
        self.ctx_stack.last_mut().unwrap()
    }

    pub(super) fn push_ctx(&mut self, ctx: HirMakerContext) {
        self.ctx_stack.push(ctx);
    }

    pub(super) fn pop_ctx(&mut self) -> HirMakerContext {
        self.ctx_stack.pop().unwrap()
    }

    /// Returns depth of next ctx
    pub(super) fn next_ctx_depth(&self) -> usize {
        self.ctx_stack.len()
    }

    pub(super) fn method_ctx(&self) -> Option<&HirMakerContext> {
        let opt_idx = self
            .find_ctx(CtxKind::Method)
            .or_else(|| self.find_ctx(CtxKind::Initializer));
        opt_idx.map(|i| &self.ctx_stack[i])
    }

    /// Find nearest enclosing ctx of the `kind`
    fn find_ctx(&self, kind: CtxKind) -> Option<usize> {
        let mut i = (self.ctx_stack.len() as isize) - 1;
        while i >= 0 {
            let ctx = &self.ctx_stack[i as usize];
            if ctx.kind == kind {
                return Some(i as usize);
            }
            i -= 1
        }
        None
    }

    pub(super) fn outer_lvar_scope_of(&self, ctx: &HirMakerContext) -> Option<&HirMakerContext> {
        if ctx.kind != CtxKind::Lambda {
            return None;
        }
        if ctx.depth == 0 {
            return None;
        }
        let outer_ctx = &self.ctx_stack[ctx.depth - 1];
        Some(outer_ctx)
    }

    /// Returns type parameter of the current class
    pub(super) fn current_class_typarams(&self) -> Vec<String> {
        let typarams = &self
            .class_dict
            .find_class(&self.ctx().self_ty.fullname)
            .unwrap()
            .typarams;
        typarams.iter().map(|x| x.name.clone()).collect()
    }

    /// Returns type parameter of the current method
    pub(super) fn current_method_typarams(&self) -> Vec<String> {
        if let Some(method_ctx) = &self.ctx.method {
            method_ctx.signature.typarams.clone()
        } else {
            vec![]
        }
    }
}
