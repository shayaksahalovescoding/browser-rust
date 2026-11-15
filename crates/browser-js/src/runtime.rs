use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use browser_dom::{Document, NodeId};

use crate::ast::{BinOp, Expr, Stmt, UnaryOp};
use crate::lexer::tokenize;
use crate::parser::Parser;

#[derive(Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Rc<RefCell<Vec<JsValue>>>),
    Object(Rc<RefCell<HashMap<String, JsValue>>>),
    Function {
        params: Vec<String>,
        body: Vec<Stmt>,
        env: Rc<RefCell<Env>>,
    },
    NativeFn(Rc<dyn Fn(&mut JsEngine, &[JsValue]) -> Result<JsValue, String>>),
    DomNode(NodeId),
    DomStyle(NodeId),
}

impl std::fmt::Debug for JsValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_display_string())
    }
}

impl JsValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Number(n) => *n != 0.0 && !n.is_nan(),
            Self::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    pub fn to_number(&self) -> f64 {
        match self {
            Self::Number(n) => *n,
            Self::Bool(true) => 1.0,
            Self::Bool(false) | Self::Null => 0.0,
            Self::String(s) => s.trim().parse().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }

    pub fn to_display_string(&self) -> String {
        match self {
            Self::Undefined => "undefined".into(),
            Self::Null => "null".into(),
            Self::Bool(b) => b.to_string(),
            Self::Number(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    n.to_string()
                }
            }
            Self::String(s) => s.clone(),
            Self::Array(a) => {
                let parts: Vec<String> = a.borrow().iter().map(|v| v.to_display_string()).collect();
                format!("[{}]", parts.join(", "))
            }
            Self::Object(_) => "[object Object]".into(),
            Self::Function { .. } | Self::NativeFn(_) => "[Function]".into(),
            Self::DomNode(id) => format!("[DomNode #{id}]"),
            Self::DomStyle(id) => format!("[CSSStyleDeclaration #{id}]"),
        }
    }
}

pub struct Env {
    vars: HashMap<String, JsValue>,
    parent: Option<Rc<RefCell<Env>>>,
}

impl Env {
    pub fn new(parent: Option<Rc<RefCell<Env>>>) -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            vars: HashMap::new(),
            parent,
        }))
    }

    pub fn get(&self, name: &str) -> JsValue {
        if let Some(v) = self.vars.get(name) {
            return v.clone();
        }
        if let Some(p) = &self.parent {
            return p.borrow().get(name);
        }
        JsValue::Undefined
    }

    pub fn define(&mut self, name: String, val: JsValue) {
        self.vars.insert(name, val);
    }

    pub fn assign(&mut self, name: &str, val: JsValue) {
        if self.vars.contains_key(name) {
            self.vars.insert(name.to_string(), val);
            return;
        }
        if let Some(p) = &self.parent {
            if p.borrow().has(name) {
                p.borrow_mut().assign(name, val);
                return;
            }
        }
        self.vars.insert(name.to_string(), val);
    }

    fn has(&self, name: &str) -> bool {
        self.vars.contains_key(name) || self.parent.as_ref().is_some_and(|p| p.borrow().has(name))
    }
}

enum ControlFlow {
    None,
    Return(JsValue),
    Break,
    Continue,
}

pub struct JsEngine {
    pub global: Rc<RefCell<Env>>,
    pub doc: Rc<RefCell<Document>>,
    pub dom_dirty: bool,
    pub console_logs: Vec<String>,
    pub listeners: HashMap<(NodeId, String), Vec<JsValue>>,
    steps: usize,
}

impl JsEngine {
    pub fn new(doc: Rc<RefCell<Document>>) -> Self {
        let global = Env::new(None);
        let mut engine = Self {
            global,
            doc,
            dom_dirty: false,
            console_logs: Vec::new(),
            listeners: HashMap::new(),
            steps: 0,
        };
        engine.install_builtins();
        engine
    }

    fn install_builtins(&mut self) {
        let console = Rc::new(RefCell::new(HashMap::new()));
        console.borrow_mut().insert(
            "log".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let line = args
                    .iter()
                    .map(|a| a.to_display_string())
                    .collect::<Vec<_>>()
                    .join(" ");
                eng.console_logs.push(line);
                Ok(JsValue::Undefined)
            })),
        );

        let math = Rc::new(RefCell::new(HashMap::new()));
        math.borrow_mut().insert(
            "floor".into(),
            JsValue::NativeFn(Rc::new(|_, a| {
                Ok(JsValue::Number(a.first().map(|v| v.to_number().floor()).unwrap_or(f64::NAN)))
            })),
        );
        math.borrow_mut().insert(
            "round".into(),
            JsValue::NativeFn(Rc::new(|_, a| {
                Ok(JsValue::Number(a.first().map(|v| v.to_number().round()).unwrap_or(f64::NAN)))
            })),
        );
        math.borrow_mut().insert(
            "max".into(),
            JsValue::NativeFn(Rc::new(|_, a| {
                let m = a.iter().map(|v| v.to_number()).fold(f64::NEG_INFINITY, f64::max);
                Ok(JsValue::Number(m))
            })),
        );

        let document = Rc::new(RefCell::new(HashMap::new()));
        document.borrow_mut().insert(
            "getElementById".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let id = args.first().map(|v| v.to_display_string()).unwrap_or_default();
                Ok(eng
                    .doc
                    .borrow()
                    .get_element_by_id(&id)
                    .map(JsValue::DomNode)
                    .unwrap_or(JsValue::Null))
            })),
        );
        document.borrow_mut().insert(
            "querySelector".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let sel = args.first().map(|v| v.to_display_string()).unwrap_or_default();
                Ok(eng
                    .doc
                    .borrow()
                    .query_selector(&sel)
                    .map(JsValue::DomNode)
                    .unwrap_or(JsValue::Null))
            })),
        );
        document.borrow_mut().insert(
            "querySelectorAll".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let sel = args.first().map(|v| v.to_display_string()).unwrap_or_default();
                let nodes: Vec<JsValue> = eng
                    .doc
                    .borrow()
                    .query_selector_all(&sel)
                    .into_iter()
                    .map(JsValue::DomNode)
                    .collect();
                Ok(JsValue::Array(Rc::new(RefCell::new(nodes))))
            })),
        );
        document.borrow_mut().insert(
            "createElement".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let tag = args.first().map(|v| v.to_display_string()).unwrap_or_else(|| "div".into());
                let id = eng.doc.borrow_mut().create_element(&tag);
                Ok(JsValue::DomNode(id))
            })),
        );
        document.borrow_mut().insert(
            "createTextNode".into(),
            JsValue::NativeFn(Rc::new(|eng, args| {
                let txt = args.first().map(|v| v.to_display_string()).unwrap_or_default();
                let id = eng.doc.borrow_mut().create_text(&txt);
                Ok(JsValue::DomNode(id))
            })),
        );

        let mut g = self.global.borrow_mut();
        g.define("console".into(), JsValue::Object(console));
        g.define("Math".into(), JsValue::Object(math));
        g.define("document".into(), JsValue::Object(document));
        g.define("window".into(), JsValue::Object(Rc::new(RefCell::new(HashMap::new()))));
    }

    pub fn eval(&mut self, source: &str) -> Result<JsValue, String> {
        self.steps = 0;
        let tokens = tokenize(source)?;
        let stmts = Parser::new(tokens).parse_program()?;
        let env = Rc::clone(&self.global);
        let mut last = JsValue::Undefined;
        for s in &stmts {
            if let Stmt::Expr(e) = s {
                last = self.eval_expr(e, &env)?;
            } else {
                last = JsValue::Undefined;
                match self.exec_stmt(s, &env)? {
                    ControlFlow::Return(v) => return Ok(v),
                    ControlFlow::None => {}
                    ControlFlow::Break | ControlFlow::Continue => break,
                }
            }
        }
        Ok(last)
    }

    pub fn dispatch_click(&mut self, node_id: NodeId) -> Result<(), String> {
        self.steps = 0;
        let mut cur = Some(node_id);
        while let Some(id) = cur {
            if let Some(fns) = self.listeners.get(&(id, "click".into())).cloned() {
                for f in fns {
                    let evt = Rc::new(RefCell::new(HashMap::new()));
                    evt.borrow_mut().insert("type".into(), JsValue::String("click".into()));
                    evt.borrow_mut().insert("target".into(), JsValue::DomNode(node_id));
                    self.call_value(f, &[JsValue::Object(evt)])?;
                }
            }
            cur = self.doc.borrow().nodes.get(id).and_then(|n| n.parent);
        }
        Ok(())
    }

    fn tick(&mut self) -> Result<(), String> {
        self.steps += 1;
        if self.steps > 200_000 {
            Err("JS execution step limit exceeded".into())
        } else {
            Ok(())
        }
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: &Rc<RefCell<Env>>) -> Result<ControlFlow, String> {
        self.tick()?;
        match stmt {
            Stmt::VarDecl { name, init } => {
                let val = match init {
                    Some(e) => self.eval_expr(e, env)?,
                    None => JsValue::Undefined,
                };
                env.borrow_mut().define(name.clone(), val);
                Ok(ControlFlow::None)
            }
            Stmt::FuncDecl { name, params, body } => {
                let f = JsValue::Function {
                    params: params.clone(),
                    body: body.clone(),
                    env: Rc::clone(env),
                };
                env.borrow_mut().define(name.clone(), f);
                Ok(ControlFlow::None)
            }
            Stmt::Block(stmts) => {
                let child = Env::new(Some(Rc::clone(env)));
                for s in stmts {
                    let cf = self.exec_stmt(s, &child)?;
                    if !matches!(cf, ControlFlow::None) {
                        return Ok(cf);
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::If {
                cond,
                then_branch,
                else_branch,
            } => {
                if self.eval_expr(cond, env)?.is_truthy() {
                    self.exec_stmt(then_branch, env)
                } else if let Some(eb) = else_branch {
                    self.exec_stmt(eb, env)
                } else {
                    Ok(ControlFlow::None)
                }
            }
            Stmt::While { cond, body } => {
                while self.eval_expr(cond, env)?.is_truthy() {
                    match self.exec_stmt(body, env)? {
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                        ControlFlow::Break => break,
                        ControlFlow::Continue | ControlFlow::None => {}
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::For {
                init,
                cond,
                step,
                body,
            } => {
                let loop_env = Env::new(Some(Rc::clone(env)));
                if let Some(i) = init {
                    self.exec_stmt(i, &loop_env)?;
                }
                loop {
                    if let Some(c) = cond {
                        if !self.eval_expr(c, &loop_env)?.is_truthy() {
                            break;
                        }
                    }
                    match self.exec_stmt(body, &loop_env)? {
                        ControlFlow::Return(v) => return Ok(ControlFlow::Return(v)),
                        ControlFlow::Break => break,
                        ControlFlow::Continue | ControlFlow::None => {}
                    }
                    if let Some(st) = step {
                        self.eval_expr(st, &loop_env)?;
                    }
                }
                Ok(ControlFlow::None)
            }
            Stmt::Return(expr) => {
                let v = match expr {
                    Some(e) => self.eval_expr(e, env)?,
                    None => JsValue::Undefined,
                };
                Ok(ControlFlow::Return(v))
            }
            Stmt::Break => Ok(ControlFlow::Break),
            Stmt::Continue => Ok(ControlFlow::Continue),
            Stmt::Expr(e) => {
                self.eval_expr(e, env)?;
                Ok(ControlFlow::None)
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: &Rc<RefCell<Env>>) -> Result<JsValue, String> {
        self.tick()?;
        match expr {
            Expr::Number(n) => Ok(JsValue::Number(*n)),
            Expr::String(s) => Ok(JsValue::String(s.clone())),
            Expr::Bool(b) => Ok(JsValue::Bool(*b)),
            Expr::Null => Ok(JsValue::Null),
            Expr::Undefined => Ok(JsValue::Undefined),
            Expr::Ident(name) => Ok(env.borrow().get(name)),
            Expr::Array(items) => {
                let mut vals = Vec::with_capacity(items.len());
                for it in items {
                    vals.push(self.eval_expr(it, env)?);
                }
                Ok(JsValue::Array(Rc::new(RefCell::new(vals))))
            }
            Expr::Object(pairs) => {
                let mut map = HashMap::new();
                for (k, v) in pairs {
                    map.insert(k.clone(), self.eval_expr(v, env)?);
                }
                Ok(JsValue::Object(Rc::new(RefCell::new(map))))
            }
            Expr::Function { params, body } => Ok(JsValue::Function {
                params: params.clone(),
                body: body.clone(),
                env: Rc::clone(env),
            }),
            Expr::Unary { op, expr } => {
                let v = self.eval_expr(expr, env)?;
                Ok(match op {
                    UnaryOp::Neg => JsValue::Number(-v.to_number()),
                    UnaryOp::Not => JsValue::Bool(!v.is_truthy()),
                    UnaryOp::Typeof => JsValue::String(
                        match v {
                            JsValue::Undefined => "undefined",
                            JsValue::Bool(_) => "boolean",
                            JsValue::Number(_) => "number",
                            JsValue::String(_) => "string",
                            JsValue::Function { .. } | JsValue::NativeFn(_) => "function",
                            _ => "object",
                        }
                        .into(),
                    ),
                })
            }
            Expr::Binary { op, left, right } => {
                if *op == BinOp::And {
                    let l = self.eval_expr(left, env)?;
                    return if !l.is_truthy() {
                        Ok(l)
                    } else {
                        self.eval_expr(right, env)
                    };
                }
                if *op == BinOp::Or {
                    let l = self.eval_expr(left, env)?;
                    return if l.is_truthy() {
                        Ok(l)
                    } else {
                        self.eval_expr(right, env)
                    };
                }
                let l = self.eval_expr(left, env)?;
                let r = self.eval_expr(right, env)?;
                Ok(eval_binary(*op, l, r))
            }
            Expr::Ternary {
                cond,
                then_e,
                else_e,
            } => {
                if self.eval_expr(cond, env)?.is_truthy() {
                    self.eval_expr(then_e, env)
                } else {
                    self.eval_expr(else_e, env)
                }
            }
            Expr::Assign { target, value } => {
                let val = self.eval_expr(value, env)?;
                match target.as_ref() {
                    Expr::Ident(name) => {
                        env.borrow_mut().assign(name, val.clone());
                    }
                    Expr::Member { object, property } => {
                        let obj = self.eval_expr(object, env)?;
                        self.set_property(&obj, property, val.clone())?;
                    }
                    Expr::Index { object, index } => {
                        let obj = self.eval_expr(object, env)?;
                        let idx = self.eval_expr(index, env)?;
                        if let JsValue::Array(arr) = obj {
                            let i = idx.to_number() as usize;
                            let mut a = arr.borrow_mut();
                            if i < a.len() {
                                a[i] = val.clone();
                            }
                        } else if let JsValue::Object(map) = obj {
                            map.borrow_mut().insert(idx.to_display_string(), val.clone());
                        }
                    }
                    _ => return Err("invalid assignment target".into()),
                }
                Ok(val)
            }
            Expr::Member { object, property } => {
                let obj = self.eval_expr(object, env)?;
                Ok(self.get_property(&obj, property))
            }
            Expr::Index { object, index } => {
                let obj = self.eval_expr(object, env)?;
                let idx = self.eval_expr(index, env)?;
                match obj {
                    JsValue::Array(arr) => {
                        let i = idx.to_number() as usize;
                        Ok(arr.borrow().get(i).cloned().unwrap_or(JsValue::Undefined))
                    }
                    JsValue::Object(map) => Ok(map
                        .borrow()
                        .get(&idx.to_display_string())
                        .cloned()
                        .unwrap_or(JsValue::Undefined)),
                    _ => Ok(JsValue::Undefined),
                }
            }
            Expr::Call { callee, args } => {
                let func = self.eval_expr(callee, env)?;
                let mut evaluated = Vec::with_capacity(args.len());
                for a in args {
                    evaluated.push(self.eval_expr(a, env)?);
                }
                self.call_value(func, &evaluated)
            }
        }
    }

    pub fn call_value(&mut self, func: JsValue, args: &[JsValue]) -> Result<JsValue, String> {
        match func {
            JsValue::NativeFn(f) => f(self, args),
            JsValue::Function { params, body, env } => {
                let fn_env = Env::new(Some(env));
                for (i, p) in params.iter().enumerate() {
                    fn_env
                        .borrow_mut()
                        .define(p.clone(), args.get(i).cloned().unwrap_or(JsValue::Undefined));
                }
                for s in &body {
                    if let ControlFlow::Return(v) = self.exec_stmt(s, &fn_env)? {
                        return Ok(v);
                    }
                }
                Ok(JsValue::Undefined)
            }
            other => Err(format!("not a function: {}", other.to_display_string())),
        }
    }

    fn get_property(&self, obj: &JsValue, prop: &str) -> JsValue {
        match obj {
            JsValue::Object(map) => map
                .borrow()
                .get(prop)
                .cloned()
                .unwrap_or(JsValue::Undefined),
            JsValue::Array(arr) => match prop {
                "length" => JsValue::Number(arr.borrow().len() as f64),
                "push" => {
                    let arr_c = Rc::clone(arr);
                    JsValue::NativeFn(Rc::new(move |_, args| {
                        let mut a = arr_c.borrow_mut();
                        a.extend_from_slice(args);
                        Ok(JsValue::Number(a.len() as f64))
                    }))
                }
                "join" => {
                    let arr_c = Rc::clone(arr);
                    JsValue::NativeFn(Rc::new(move |_, args| {
                        let sep = args
                            .first()
                            .map(|v| v.to_display_string())
                            .unwrap_or_else(|| ",".into());
                        let joined = arr_c
                            .borrow()
                            .iter()
                            .map(|v| v.to_display_string())
                            .collect::<Vec<_>>()
                            .join(&sep);
                        Ok(JsValue::String(joined))
                    }))
                }
                _ => JsValue::Undefined,
            },
            JsValue::String(s) => match prop {
                "length" => JsValue::Number(s.chars().count() as f64),
                "toUpperCase" => {
                    let sc = s.clone();
                    JsValue::NativeFn(Rc::new(move |_, _| Ok(JsValue::String(sc.to_uppercase()))))
                }
                "toLowerCase" => {
                    let sc = s.clone();
                    JsValue::NativeFn(Rc::new(move |_, _| Ok(JsValue::String(sc.to_lowercase()))))
                }
                "trim" => {
                    let sc = s.clone();
                    JsValue::NativeFn(Rc::new(move |_, _| {
                        Ok(JsValue::String(sc.trim().to_string()))
                    }))
                }
                _ => JsValue::Undefined,
            },
            JsValue::DomNode(id) => {
                let id = *id;
                match prop {
                    "textContent" | "innerText" => {
                        JsValue::String(self.doc.borrow().text_content(id))
                    }
                    "style" => JsValue::DomStyle(id),
                    "appendChild" => JsValue::NativeFn(Rc::new(move |eng, args| {
                        if let Some(JsValue::DomNode(child_id)) = args.first() {
                            eng.doc.borrow_mut().append_child(id, *child_id);
                            eng.dom_dirty = true;
                            return Ok(JsValue::DomNode(*child_id));
                        }
                        Ok(JsValue::Undefined)
                    })),
                    "setAttribute" => JsValue::NativeFn(Rc::new(move |eng, args| {
                        if args.len() >= 2 {
                            eng.doc.borrow_mut().set_attribute(
                                id,
                                &args[0].to_display_string(),
                                &args[1].to_display_string(),
                            );
                            eng.dom_dirty = true;
                        }
                        Ok(JsValue::Undefined)
                    })),
                    "getAttribute" => JsValue::NativeFn(Rc::new(move |eng, args| {
                        let key = args.first().map(|v| v.to_display_string()).unwrap_or_default();
                        Ok(eng
                            .doc
                            .borrow()
                            .get_attribute(id, &key)
                            .map(|s| JsValue::String(s.to_string()))
                            .unwrap_or(JsValue::Null))
                    })),
                    "addEventListener" => JsValue::NativeFn(Rc::new(move |eng, args| {
                        if args.len() >= 2 {
                            let ev = args[0].to_display_string();
                            eng.listeners
                                .entry((id, ev))
                                .or_default()
                                .push(args[1].clone());
                        }
                        Ok(JsValue::Undefined)
                    })),
                    _ => JsValue::Undefined,
                }
            }
            _ => JsValue::Undefined,
        }
    }

    fn set_property(&mut self, obj: &JsValue, prop: &str, val: JsValue) -> Result<(), String> {
        match obj {
            JsValue::Object(map) => {
                map.borrow_mut().insert(prop.to_string(), val);
            }
            JsValue::DomNode(id) => {
                if prop == "textContent" || prop == "innerText" {
                    self.doc
                        .borrow_mut()
                        .set_text_content(*id, &val.to_display_string());
                    self.dom_dirty = true;
                }
            }
            JsValue::DomStyle(id) => {
                let css_prop = camel_to_kebab(prop);
                let css_val = val.to_display_string();
                let mut doc = self.doc.borrow_mut();
                let mut existing = doc.get_attribute(*id, "style").unwrap_or("").to_string();
                if !existing.is_empty() && !existing.trim_end().ends_with(';') {
                    existing.push(';');
                }
                existing.push_str(&format!(" {css_prop}: {css_val};"));
                doc.set_attribute(*id, "style", &existing);
                self.dom_dirty = true;
            }
            _ => {}
        }
        Ok(())
    }
}

fn camel_to_kebab(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch.is_ascii_uppercase() {
            out.push('-');
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn eval_binary(op: BinOp, l: JsValue, r: JsValue) -> JsValue {
    match op {
        BinOp::Add => match (&l, &r) {
            (JsValue::String(a), _) => JsValue::String(format!("{a}{}", r.to_display_string())),
            (_, JsValue::String(b)) => JsValue::String(format!("{}{b}", l.to_display_string())),
            _ => JsValue::Number(l.to_number() + r.to_number()),
        },
        BinOp::Sub => JsValue::Number(l.to_number() - r.to_number()),
        BinOp::Mul => JsValue::Number(l.to_number() * r.to_number()),
        BinOp::Div => JsValue::Number(l.to_number() / r.to_number()),
        BinOp::Mod => JsValue::Number(l.to_number() % r.to_number()),
        BinOp::EqEq | BinOp::EqEqEq => JsValue::Bool(js_equal(&l, &r)),
        BinOp::NotEq | BinOp::NotEqEq => JsValue::Bool(!js_equal(&l, &r)),
        BinOp::Lt => JsValue::Bool(l.to_number() < r.to_number()),
        BinOp::Le => JsValue::Bool(l.to_number() <= r.to_number()),
        BinOp::Gt => JsValue::Bool(l.to_number() > r.to_number()),
        BinOp::Ge => JsValue::Bool(l.to_number() >= r.to_number()),
        BinOp::And | BinOp::Or => JsValue::Undefined,
    }
}

fn js_equal(a: &JsValue, b: &JsValue) -> bool {
    match (a, b) {
        (JsValue::Number(x), JsValue::Number(y)) => x == y,
        (JsValue::String(x), JsValue::String(y)) => x == y,
        (JsValue::Bool(x), JsValue::Bool(y)) => x == y,
        (JsValue::Null, JsValue::Null) | (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::DomNode(x), JsValue::DomNode(y)) => x == y,
        _ => false,
    }
}
