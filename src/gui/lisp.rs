//! A tiny embedded Lisp — parasite's "brain without AI". The graph advisor runs a
//! set of hand-written Lisp rules over deterministic facts about the current graph
//! and emits actionable suggestions. No model, no network: pure, explainable
//! heuristics you can read and edit (see `RULES`).
//!
//! The interpreter supports just what the rules need: `when`, `if`, `and`, `or`,
//! `not`, comparisons/arithmetic, a handful of graph-query primitives, and
//! `(suggest "title" "detail")`.

use std::collections::HashMap;

/// Deterministic facts computed from the graph, queried by the rules.
#[derive(Default)]
pub struct Facts {
    /// short kind tag ("domain","ip",…) → node count
    pub kinds:      HashMap<String, usize>,
    pub nodes:      usize,
    pub edges:      usize,
    pub isolated:   usize, // nodes with no edges
    pub leaves:     usize, // nodes with exactly one edge (likely unexpanded)
    pub flagged:    usize,
    pub max_degree: usize, // highest node degree (a "hub")
    pub hubs:       usize, // nodes with degree >= 5
    pub components: usize, // connected components (separate clusters)
    pub cycles:     usize, // independent cycles: edges - nodes + components
    pub duplicates: usize, // values that appear on more than one node
    pub distinct:   usize, // distinct entity kinds present
    pub avg_degree: f64,   // 2*edges / nodes
    pub cohosted:   usize, // domains sharing an IP with another domain (co-hosting)
    pub biggest:    usize, // size of the largest connected component
    /// property key → size of the largest group of nodes sharing one value
    pub shared:     HashMap<String, usize>,
    /// "kind|transform-id" → how many nodes of that kind have NOT run it (coverage)
    pub unrun:      HashMap<String, usize>,
    /// property key → how many nodes carry that property at all
    pub props:      HashMap<String, usize>,
}

impl Facts {
    fn num(&self, name: &str, arg: Option<&str>) -> Option<f64> {
        Some(match name {
            "count-nodes"     => self.nodes as f64,
            "count-edges"     => self.edges as f64,
            "isolated-count"  => self.isolated as f64,
            "leaf-count"      => self.leaves as f64,
            "flagged-count"   => self.flagged as f64,
            "max-degree"      => self.max_degree as f64,
            "hub-count"       => self.hubs as f64,
            "component-count" => self.components as f64,
            "cycle-count"     => self.cycles as f64,
            "dup-count"       => self.duplicates as f64,
            "distinct-kinds"  => self.distinct as f64,
            "avg-degree"      => self.avg_degree,
            "cohosted-count"  => self.cohosted as f64,
            "biggest-cluster" => self.biggest as f64,
            "count-kind"      => *self.kinds.get(arg?).unwrap_or(&0) as f64,
            "shared-prop"     => *self.shared.get(arg?).unwrap_or(&0) as f64,
            "count-prop"      => *self.props.get(arg?).unwrap_or(&0) as f64,
            _ => return None,
        })
    }
}

/// A suggestion the advisor produced. `select` (e.g. "kind:ip", "isolated",
/// "shared:registrar") tells the UI which nodes to highlight; `action`
/// (e.g. "run:dom_resolve", "machine:Domain Deep Recon") makes it one-click.
#[derive(Clone, Default)]
pub struct Suggestion {
    pub title:  String,
    pub detail: String,
    pub select: String,
    pub action: String,
    /// 0 = info, 1 = tip, 2 = warning — drives the colour in the panel.
    pub level:  u8,
    /// human-readable "why this fired": the facts the rule's condition tested.
    pub explain: String,
}

// ── values + parser ───────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Val { Nil, Bool(bool), Num(f64), Str(String), Sym(String), List(Vec<Val>) }

fn tokenize(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        match c {
            ';' => { while let Some(&n) = chars.peek() { if n == '\n' { break; } chars.next(); } }
            '(' | ')' => { out.push(c.to_string()); chars.next(); }
            '"' => {
                chars.next();
                let mut s = String::from("\"");
                while let Some(&n) = chars.peek() { chars.next(); if n == '"' { break; } s.push(n); }
                out.push(s);
            }
            c if c.is_whitespace() => { chars.next(); }
            _ => {
                let mut tok = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '(' || n == ')' { break; }
                    tok.push(n); chars.next();
                }
                out.push(tok);
            }
        }
    }
    out
}

fn parse_all(tokens: &[String]) -> Vec<Val> {
    let mut pos = 0;
    let mut forms = Vec::new();
    while pos < tokens.len() {
        if let Some(v) = parse(tokens, &mut pos) { forms.push(v); } else { break; }
    }
    forms
}

fn parse(tokens: &[String], pos: &mut usize) -> Option<Val> {
    if *pos >= tokens.len() { return None; }
    let t = &tokens[*pos];
    *pos += 1;
    match t.as_str() {
        "(" => {
            let mut items = Vec::new();
            while *pos < tokens.len() && tokens[*pos] != ")" {
                if let Some(v) = parse(tokens, pos) { items.push(v); } else { break; }
            }
            if *pos < tokens.len() { *pos += 1; } // consume ")"
            Some(Val::List(items))
        }
        ")" => None,
        s if s.starts_with('"') => Some(Val::Str(s[1..].to_string())),
        "true"  => Some(Val::Bool(true)),
        "false" => Some(Val::Bool(false)),
        "nil"   => Some(Val::Nil),
        s => match s.parse::<f64>() { Ok(n) => Some(Val::Num(n)), Err(_) => Some(Val::Sym(s.to_string())) },
    }
}

// ── evaluator ─────────────────────────────────────────────────────────────────

struct Ctx<'a> { facts: &'a Facts, out: Vec<Suggestion>, env: HashMap<String, Val>, why: String }

/// Fact-query function names (used by the "explain" tracer).
const FACT_FNS: &[&str] = &["count-nodes","count-edges","count-kind","has-kind","any-kind",
    "isolated-count","leaf-count","hub-count","max-degree","component-count","cycle-count",
    "dup-count","distinct-kinds","avg-degree","cohosted-count","biggest-cluster","shared-prop",
    "unrun","flagged-count","count-prop","pct","ratio"];

/// Walk a condition expression and collect "fact = value" terms for the explainer.
fn explain_cond(e: &Val, facts: &Facts, out: &mut Vec<String>) {
    if let Val::List(items) = e {
        if let Some(Val::Sym(op)) = items.first() {
            if FACT_FNS.contains(&op.as_str()) {
                let args: Vec<String> = items[1..].iter().filter_map(|a| match a {
                    Val::Str(s) => Some(s.clone()), Val::Num(n) => Some(format!("{n}")), _ => None,
                }).collect();
                let val = if *op == "has-kind" {
                    format!("{}", facts.num("count-kind", args.first().map(|s| s.as_str())).unwrap_or(0.0) > 0.0)
                } else if *op == "unrun" {
                    format!("{}", *facts.unrun.get(&format!("{}|{}", args.first().cloned().unwrap_or_default(),
                        args.get(1).cloned().unwrap_or_default())).unwrap_or(&0))
                } else {
                    let v = facts.num(op, args.first().map(|s| s.as_str())).unwrap_or(0.0);
                    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v:.1}") }
                };
                let a = if args.is_empty() { String::new() } else { format!(" {}", args.join(" ")) };
                out.push(format!("{op}{a} = {val}"));
                return;
            }
        }
        for it in items { explain_cond(it, facts, out); }
    }
}

fn truthy(v: &Val) -> bool {
    match v { Val::Bool(b) => *b, Val::Nil => false, Val::Num(n) => *n != 0.0,
              Val::Str(s) => !s.is_empty(), Val::List(l) => !l.is_empty(), Val::Sym(_) => true }
}
fn as_num(v: &Val) -> f64 { match v { Val::Num(n) => *n, Val::Bool(b) => *b as i32 as f64, _ => 0.0 } }
fn as_str(v: &Val) -> String { match v { Val::Str(s) => s.clone(), Val::Sym(s) => s.clone(),
                                         Val::Num(n) => n.to_string(), _ => String::new() } }

fn eval(e: &Val, ctx: &mut Ctx) -> Val {
    // a bare symbol resolves to a `define`d variable (else nil)
    if let Val::Sym(s) = e { return ctx.env.get(s).cloned().unwrap_or(Val::Nil); }
    let items = match e { Val::List(i) if !i.is_empty() => i, _ => return e.clone() };
    let op = match &items[0] { Val::Sym(s) => s.as_str(), _ => return Val::Nil };
    let args = &items[1..];

    match op {
        "define" => {
            if let (Some(Val::Sym(name)), Some(v)) = (items.get(1), args.get(1)) {
                let val = eval(v, ctx);
                ctx.env.insert(name.clone(), val);
            }
            Val::Nil
        }
        "min" | "max" => {
            let mut it = args.iter().map(|a| as_num(&eval(a, ctx)));
            let mut acc = it.next().unwrap_or(0.0);
            for n in it { acc = if op == "min" { acc.min(n) } else { acc.max(n) }; }
            Val::Num(acc)
        }
        "between" => { // (between x lo hi)
            let x  = as_num(&eval(&args[0], ctx));
            let lo = as_num(&eval(&args[1], ctx));
            let hi = as_num(&eval(&args[2], ctx));
            Val::Bool(x >= lo && x <= hi)
        }
        "ratio" => { // (ratio a b) = a/b (0 if b==0)
            let a = as_num(&eval(&args[0], ctx));
            let b = as_num(&eval(&args[1], ctx));
            Val::Num(if b == 0.0 { 0.0 } else { a / b })
        }
        "pct" => { // (pct part whole) = part/whole*100
            let a = as_num(&eval(&args[0], ctx));
            let b = as_num(&eval(&args[1], ctx));
            Val::Num(if b == 0.0 { 0.0 } else { a / b * 100.0 })
        }
        "any-kind" => Val::Bool(args.iter()
            .any(|a| ctx.facts.num("count-kind", Some(&as_str(&eval(a, ctx)))).unwrap_or(0.0) > 0.0)),
        "unrun" => { // (unrun "ip" "ip_greynoise") → nodes of kind without that check
            let k = as_str(&eval(&args[0], ctx));
            let t = as_str(&eval(&args[1], ctx));
            Val::Num(*ctx.facts.unrun.get(&format!("{k}|{t}")).unwrap_or(&0) as f64)
        }
        "when" => {
            if !args.is_empty() && truthy(&eval(&args[0], ctx)) {
                let mut terms = Vec::new();
                explain_cond(&args[0], ctx.facts, &mut terms);
                let prev = std::mem::replace(&mut ctx.why, terms.join(" · "));
                for a in &args[1..] { eval(a, ctx); }
                ctx.why = prev;
            }
            Val::Nil
        }
        "if" => {
            if !args.is_empty() && truthy(&eval(&args[0], ctx)) {
                args.get(1).map(|a| eval(a, ctx)).unwrap_or(Val::Nil)
            } else {
                args.get(2).map(|a| eval(a, ctx)).unwrap_or(Val::Nil)
            }
        }
        "and" => Val::Bool(args.iter().all(|a| truthy(&eval(a, ctx)))),
        "or"  => Val::Bool(args.iter().any(|a| truthy(&eval(a, ctx)))),
        "not" => Val::Bool(!args.first().map(|a| truthy(&eval(a, ctx))).unwrap_or(false)),
        "=" | ">" | "<" | ">=" | "<=" => {
            let a = eval(&args[0], ctx); let b = eval(&args[1], ctx);
            let (x, y) = (as_num(&a), as_num(&b));
            Val::Bool(match op { "=" => (x - y).abs() < f64::EPSILON, ">" => x > y, "<" => x < y,
                                 ">=" => x >= y, _ => x <= y })
        }
        "+" | "-" | "*" | "/" => {
            let nums: Vec<f64> = args.iter().map(|a| as_num(&eval(a, ctx))).collect();
            let mut acc = nums.first().copied().unwrap_or(0.0);
            for n in &nums[1..] { match op { "+" => acc += n, "-" => acc -= n, "*" => acc *= n,
                                             _ => if *n != 0.0 { acc /= n } } }
            Val::Num(acc)
        }
        "has-kind" => Val::Bool(ctx.facts.num("count-kind", Some(&as_str(&eval(&args[0], ctx)))).unwrap_or(0.0) > 0.0),
        "suggest" | "suggest-run" | "tip" | "warn" => {
            // (suggest|tip|warn title detail [select action])
            let title  = args.first().map(|a| as_str(&eval(a, ctx))).unwrap_or_default();
            let detail = args.get(1).map(|a| as_str(&eval(a, ctx))).unwrap_or_default();
            let select = args.get(2).map(|a| as_str(&eval(a, ctx))).unwrap_or_default();
            let action = args.get(3).map(|a| as_str(&eval(a, ctx))).unwrap_or_default();
            let level = match op { "warn" => 2, "tip" => 1, _ => 0 };
            let explain = ctx.why.clone();
            ctx.out.push(Suggestion { title, detail, select, action, level, explain });
            Val::Nil
        }
        // graph-query primitives that take an optional string arg
        name => {
            let arg = args.first().map(|a| as_str(&eval(a, ctx)));
            ctx.facts.num(name, arg.as_deref()).map(Val::Num).unwrap_or(Val::Nil)
        }
    }
}

/// Parse + run the built-in rule program against `facts`.
#[allow(dead_code)]
pub fn advise(facts: &Facts) -> Vec<Suggestion> {
    advise_with(RULES, facts)
}

/// Parse + run a custom rule program (for the in-app, live-reloaded editor).
pub fn advise_with(rules: &str, facts: &Facts) -> Vec<Suggestion> {
    let forms = parse_all(&tokenize(rules));
    let mut ctx = Ctx { facts, out: Vec::new(), env: HashMap::new(), why: String::new() };
    for f in &forms { eval(f, &mut ctx); }
    // priority: warnings first, then tips, then info — stable within a level
    ctx.out.sort_by(|a, b| b.level.cmp(&a.level));
    ctx.out
}

/// The advisor rule-set. Plain Lisp — readable and editable. Each rule emits a
/// suggestion when its condition over the current graph holds.
pub const RULES: &str = r#"
; ╔══════════════════════════════════════════════════════════════╗
; ║  λ INSTINCT — parasite's rule brain. Plain Lisp, no AI.       ║
; ╚══════════════════════════════════════════════════════════════╝
; Forms:
;   (suggest title detail)                  -> info hint
;   (tip     title detail)                  -> blue tip
;   (warn    title detail)                  -> orange warning
;   (suggest-run title detail select action)-> one-click hint
;     select = "kind:ip" | "isolated" | "leaves" | "hubs"
;              | "flagged" | "shared:registrar"
;     action = "run:<transform-id>" | "machine:<Name>"
; Facts: count-nodes count-edges count-kind has-kind any-kind isolated-count
;   leaf-count hub-count max-degree component-count cycle-count dup-count
;   distinct-kinds avg-degree cohosted-count biggest-cluster flagged-count
;   shared-prop  (unrun "kind" "transform-id")  define min max between

; ── coverage gaps: checks you never ran (ties into the Coverage board) ──
(when (>= (unrun "ip" "ip_greynoise") 1)
  (suggest-run "Unchecked IP reputation" "Some IPs never went through GreyNoise — triage them." "kind:ip" "run:ip_greynoise"))
(when (>= (unrun "email" "email_hibp") 1)
  (suggest-run "Emails not checked for breaches" "Run Have I Been Pwned on the un-checked emails." "kind:email" "run:email_hibp"))
(when (>= (unrun "domain" "dom_whois") 1)
  (suggest-run "Missing WHOIS" "Pull WHOIS on domains you haven't enriched — it ties them to an owner." "kind:domain" "run:dom_whois"))

; ── structural correlation: strong same-actor signals ──
(when (>= (cohosted-count) 2)
  (warn "Co-hosted domains" "Domains share an IP — likely the same operator or hosting reseller."))
(when (>= (biggest-cluster) 15)
  (tip "One dominant cluster" "A single large cluster carries most of the case — focus your pivots there."))
(when (and (any-kind "btc" "eth") (= (count-kind "tx") 0))
  (suggest "Trace the wallet" "You have a crypto address but no transactions — trace its money flow."))
(when (> (pct (count-kind "ip") (count-nodes)) 55)
  (tip "Infrastructure-heavy" "Most of the graph is IPs — pivot to owners, domains and certs for context."))
(when (>= (count-prop "registrar") 3)
  (tip "Registrant data present" "Several nodes carry WHOIS — cluster them by registrant to spot one owner."))

; ── coverage: things you collected but haven't expanded ──
(when (and (has-kind "domain") (= (count-kind "ip") 0))
  (suggest-run "Resolve your domains"
               "You have domains but no IPs yet — map their infrastructure."
               "kind:domain" "run:dom_resolve"))

(when (and (has-kind "ip") (= (count-kind "port") 0) (= (count-kind "service") 0))
  (suggest-run "Probe the hosts"
               "IPs present but no ports/services — query Shodan InternetDB."
               "kind:ip" "run:ip_internetdb"))

(when (and (has-kind "email") (= (count-kind "username") 0))
  (suggest-run "Pivot from emails"
               "Extract usernames from emails and hunt for matching accounts."
               "kind:email" "run:mail_user"))

(when (and (has-kind "username") (= (count-kind "social") 0))
  (suggest-run "Find social profiles"
               "Usernames but no social profiles — run the account hunt."
               "kind:username" "run:user_hunt"))

(when (and (has-kind "domain") (= (count-kind "domain") 1) (= (count-edges) 0))
  (suggest-run "Run a deep recon machine"
               "Single domain, nothing expanded — kick off the full recon pipeline."
               "kind:domain" "machine:Domain Deep Recon"))

(when (and (has-kind "person") (= (count-kind "username") 0) (= (count-kind "email") 0))
  (suggest "Anchor the person"
           "A person with no email/username is hard to pivot — find identifiers first."))

; ── structure / hygiene ──
(when (> (isolated-count) 2)
  (suggest-run "Connect isolated nodes"
               "Several nodes have no links — expand them to reveal relationships."
               "isolated" ""))

(when (> (leaf-count) 5)
  (suggest-run "Expand leaf nodes"
               "Many nodes sit at the edge with one link — grow them with transforms."
               "leaves" ""))

(when (>= (max-degree) 6)
  (suggest-run "Inspect the hubs"
               "Highly-connected nodes are central to this case — review them first."
               "hubs" ""))

(when (> (component-count) 3)
  (suggest "Several disconnected clusters"
           "The graph has separate islands — look for links that bridge them."))

(when (>= (cycle-count) 1)
  (tip "Closed loops detected"
       "Cycles mean cross-links between entities — often a strong correlation."))

(when (>= (dup-count) 1)
  (warn "Duplicate values"
        "The same value appears on multiple nodes — merge or verify them."))

(when (and (> (count-nodes) 8) (< (avg-degree) 1.2))
  (warn "Sparse graph"
        "Lots of nodes but few links — expand entities so relationships emerge."))

(when (>= (distinct-kinds) 6)
  (tip "Rich entity mix"
       "Many entity types in play — a good case for the Advisor's pivots and machines."))

; ── correlation: shared attributes hint at the same actor ──
(when (>= (shared-prop "registrar") 2)
  (suggest-run "Shared registrar detected"
               "Multiple domains share one registrant — likely the same owner."
               "shared:registrar" ""))

(when (>= (shared-prop "org") 2)
  (suggest-run "Shared hosting / org"
               "Several hosts share an organisation — common infrastructure."
               "shared:org" ""))

(when (>= (shared-prop "country") 3)
  (suggest "Geographic cluster"
           "Many entities share a country — consider the GEOINT view."))

; ── triage / workflow ──
(when (and (> (count-kind "ip") 0) (= (flagged-count) 0))
  (suggest-run "Triage and flag"
               "Run reputation checks and flag risky hosts."
               "kind:ip" "run:ip_greynoise"))

(when (> (count-nodes) 40)
  (suggest "Save this investigation"
           "The graph is getting large — save it as a Case and consider clustering."))

(when (and (> (count-nodes) 0) (= (count-edges) 0))
  (suggest "Start expanding"
           "Entities but no connections yet — right-click a node and run a transform."))
"#;

/// Russian rule-set (same logic, localized text).
pub const RULES_RU: &str = r#"
; λ ИНСТИНКТ — мозг паразита на правилах. Чистый Lisp, без ИИ.
(when (>= (unrun "ip" "ip_greynoise") 1)
  (suggest-run "IP без проверки репутации" "Часть IP не прогнали через GreyNoise — сделайте триаж." "kind:ip" "run:ip_greynoise"))
(when (>= (unrun "email" "email_hibp") 1)
  (suggest-run "Почты без проверки утечек" "Прогоните Have I Been Pwned по непроверенным почтам." "kind:email" "run:email_hibp"))
(when (>= (unrun "domain" "dom_whois") 1)
  (suggest-run "Нет WHOIS" "Снимите WHOIS с доменов — это привяжет их к владельцу." "kind:domain" "run:dom_whois"))
(when (>= (cohosted-count) 2)
  (warn "Совместный хостинг" "Домены делят один IP — вероятно один оператор или реселлер."))
(when (>= (biggest-cluster) 15)
  (tip "Один доминирующий кластер" "Основная часть дела в одном большом кластере — копайте там."))
(when (and (any-kind "btc" "eth") (= (count-kind "tx") 0))
  (suggest "Проследите кошелёк" "Есть крипто-адрес, но нет транзакций — отследите поток средств."))
(when (> (pct (count-kind "ip") (count-nodes)) 55)
  (tip "Перекос в инфраструктуру" "Граф в основном из IP — добавьте владельцев, домены и сертификаты для контекста."))
(when (>= (count-prop "registrar") 3)
  (tip "Есть данные регистратора" "У нескольких узлов есть WHOIS — сгруппируйте по регистратору, чтобы найти владельца."))
(when (and (has-kind "domain") (= (count-kind "ip") 0))
  (suggest-run "Резолвьте домены" "Есть домены, но нет IP — раскройте инфраструктуру." "kind:domain" "run:dom_resolve"))
(when (and (has-kind "ip") (= (count-kind "port") 0) (= (count-kind "service") 0))
  (suggest-run "Прозвоните хосты" "Есть IP, но нет портов/сервисов — спросите Shodan InternetDB." "kind:ip" "run:ip_internetdb"))
(when (and (has-kind "email") (= (count-kind "username") 0))
  (suggest-run "Пивот с почт" "Достаньте ники из почт и поищите аккаунты." "kind:email" "run:mail_user"))
(when (and (has-kind "username") (= (count-kind "social") 0))
  (suggest-run "Найдите соцпрофили" "Есть ники, но нет соцсетей — запустите охоту по аккаунтам." "kind:username" "run:user_hunt"))
(when (and (has-kind "domain") (= (count-kind "domain") 1) (= (count-edges) 0))
  (suggest-run "Запустите машину разведки" "Один домен и ничего не раскрыто — запустите полный пайплайн." "kind:domain" "machine:Domain Deep Recon"))
(when (and (has-kind "person") (= (count-kind "username") 0) (= (count-kind "email") 0))
  (suggest "Закрепите личность" "Человек без почты/ника плохо пивотится — сначала найдите идентификаторы."))
(when (> (isolated-count) 2)
  (suggest-run "Свяжите одинокие узлы" "У нескольких узлов нет связей — раскройте их." "isolated" ""))
(when (> (leaf-count) 5)
  (suggest-run "Раскройте листовые узлы" "Много узлов с одной связью — прогоните трансформы." "leaves" ""))
(when (>= (max-degree) 6)
  (suggest-run "Осмотрите хабы" "Сильно связанные узлы — ключ к делу, начните с них." "hubs" ""))
(when (> (component-count) 3)
  (suggest "Несколько отдельных кластеров" "В графе есть острова — поищите связи-мосты между ними."))
(when (>= (cycle-count) 1)
  (tip "Найдены замкнутые петли" "Циклы — это перекрёстные связи, часто сильная корреляция."))
(when (>= (dup-count) 1)
  (warn "Дубликаты значений" "Одно значение на нескольких узлах — объедините или проверьте."))
(when (and (> (count-nodes) 8) (< (avg-degree) 1.2))
  (warn "Разреженный граф" "Много узлов, мало связей — раскрывайте сущности."))
(when (>= (distinct-kinds) 6)
  (tip "Богатый набор типов" "Много типов сущностей — самое время для пивотов и машин."))
(when (>= (shared-prop "registrar") 2)
  (suggest-run "Общий регистратор" "Несколько доменов делят регистратора — вероятно один владелец." "shared:registrar" ""))
(when (>= (shared-prop "org") 2)
  (suggest-run "Общий хостинг/орг" "Хосты делят организацию — общая инфраструктура." "shared:org" ""))
(when (>= (shared-prop "country") 3)
  (suggest "Гео-кластер" "Много сущностей в одной стране — гляньте режим GEOINT."))
(when (and (> (count-kind "ip") 0) (= (flagged-count) 0))
  (suggest-run "Триаж и флажки" "Прогоните проверки репутации и пометьте рискованные хосты." "kind:ip" "run:ip_greynoise"))
(when (> (count-nodes) 40)
  (suggest "Сохраните расследование" "Граф разрастается — сохраните как Дело и подумайте о кластеризации."))
(when (and (> (count-nodes) 0) (= (count-edges) 0))
  (suggest "Начните раскрывать" "Сущности есть, связей нет — ПКМ по узлу и запустите трансформ."))
"#;

/// Ukrainian rule-set (same logic, localized text).
pub const RULES_UK: &str = r#"
; λ ІНСТИНКТ — мозок паразита на правилах. Чистий Lisp, без ШІ.
(when (>= (unrun "ip" "ip_greynoise") 1)
  (suggest-run "IP без перевірки репутації" "Частину IP не прогнали через GreyNoise — зробіть тріаж." "kind:ip" "run:ip_greynoise"))
(when (>= (unrun "email" "email_hibp") 1)
  (suggest-run "Пошти без перевірки витоків" "Проженіть Have I Been Pwned по неперевірених поштах." "kind:email" "run:email_hibp"))
(when (>= (unrun "domain" "dom_whois") 1)
  (suggest-run "Немає WHOIS" "Зніміть WHOIS з доменів — це прив'яже їх до власника." "kind:domain" "run:dom_whois"))
(when (>= (cohosted-count) 2)
  (warn "Спільний хостинг" "Домени ділять один IP — ймовірно один оператор або реселер."))
(when (>= (biggest-cluster) 15)
  (tip "Один домінуючий кластер" "Основна частина справи в одному великому кластері — копайте там."))
(when (and (any-kind "btc" "eth") (= (count-kind "tx") 0))
  (suggest "Простежте гаманець" "Є крипто-адреса, але немає транзакцій — відстежте потік коштів."))
(when (> (pct (count-kind "ip") (count-nodes)) 55)
  (tip "Перекіс в інфраструктуру" "Граф переважно з IP — додайте власників, домени та сертифікати для контексту."))
(when (>= (count-prop "registrar") 3)
  (tip "Є дані реєстратора" "У кількох вузлів є WHOIS — згрупуйте за реєстратором, щоб знайти власника."))
(when (and (has-kind "domain") (= (count-kind "ip") 0))
  (suggest-run "Резолвте домени" "Є домени, але немає IP — розкрийте інфраструктуру." "kind:domain" "run:dom_resolve"))
(when (and (has-kind "ip") (= (count-kind "port") 0) (= (count-kind "service") 0))
  (suggest-run "Прозвоніть хости" "Є IP, але немає портів/сервісів — спитайте Shodan InternetDB." "kind:ip" "run:ip_internetdb"))
(when (and (has-kind "email") (= (count-kind "username") 0))
  (suggest-run "Півот з пошт" "Дістаньте ніки з пошт і пошукайте акаунти." "kind:email" "run:mail_user"))
(when (and (has-kind "username") (= (count-kind "social") 0))
  (suggest-run "Знайдіть соцпрофілі" "Є ніки, але немає соцмереж — запустіть полювання за акаунтами." "kind:username" "run:user_hunt"))
(when (and (has-kind "domain") (= (count-kind "domain") 1) (= (count-edges) 0))
  (suggest-run "Запустіть машину розвідки" "Один домен і нічого не розкрито — запустіть повний пайплайн." "kind:domain" "machine:Domain Deep Recon"))
(when (and (has-kind "person") (= (count-kind "username") 0) (= (count-kind "email") 0))
  (suggest "Закріпіть особу" "Людина без пошти/ніка погано півотиться — спершу знайдіть ідентифікатори."))
(when (> (isolated-count) 2)
  (suggest-run "Зв'яжіть самотні вузли" "У кількох вузлів немає зв'язків — розкрийте їх." "isolated" ""))
(when (> (leaf-count) 5)
  (suggest-run "Розкрийте листові вузли" "Багато вузлів з одним зв'язком — проженіть трансформи." "leaves" ""))
(when (>= (max-degree) 6)
  (suggest-run "Огляньте хаби" "Сильно зв'язані вузли — ключ до справи, почніть з них." "hubs" ""))
(when (> (component-count) 3)
  (suggest "Кілька окремих кластерів" "У графі є острови — пошукайте зв'язки-мости між ними."))
(when (>= (cycle-count) 1)
  (tip "Знайдено замкнені петлі" "Цикли — це перехресні зв'язки, часто сильна кореляція."))
(when (>= (dup-count) 1)
  (warn "Дублікати значень" "Одне значення на кількох вузлах — об'єднайте або перевірте."))
(when (and (> (count-nodes) 8) (< (avg-degree) 1.2))
  (warn "Розріджений граф" "Багато вузлів, мало зв'язків — розкривайте сутності."))
(when (>= (distinct-kinds) 6)
  (tip "Багатий набір типів" "Багато типів сутностей — саме час для півотів і машин."))
(when (>= (shared-prop "registrar") 2)
  (suggest-run "Спільний реєстратор" "Кілька доменів ділять реєстратора — ймовірно один власник." "shared:registrar" ""))
(when (>= (shared-prop "org") 2)
  (suggest-run "Спільний хостинг/орг" "Хости ділять організацію — спільна інфраструктура." "shared:org" ""))
(when (>= (shared-prop "country") 3)
  (suggest "Гео-кластер" "Багато сутностей в одній країні — гляньте режим GEOINT."))
(when (and (> (count-kind "ip") 0) (= (flagged-count) 0))
  (suggest-run "Тріаж і прапорці" "Проженіть перевірки репутації та позначте ризиковані хости." "kind:ip" "run:ip_greynoise"))
(when (> (count-nodes) 40)
  (suggest "Збережіть розслідування" "Граф розростається — збережіть як Справу та подумайте про кластеризацію."))
(when (and (> (count-nodes) 0) (= (count-edges) 0))
  (suggest "Почніть розкривати" "Сутності є, зв'язків немає — ПКМ по вузлу і запустіть трансформ."))
"#;

/// The default rule program for a language.
pub fn rules_default(lang: super::i18n::Lang) -> &'static str {
    match lang { super::i18n::Lang::Ru => RULES_RU, super::i18n::Lang::Uk => RULES_UK, _ => RULES }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_parse_and_fire() {
        let mut f = Facts::default();
        f.nodes = 3;
        f.kinds.insert("domain".into(), 2);
        // no ips → "resolve your domains" should fire, with an action
        let out = advise(&f);
        let s = out.iter().find(|s| s.title.contains("Resolve")).expect("rule fired");
        assert_eq!(s.action, "run:dom_resolve");
        assert_eq!(s.select, "kind:domain");
    }

    #[test]
    fn empty_graph_is_quiet_ish() {
        let f = Facts::default();
        // should not panic and returns a (possibly empty) vec
        let _ = advise(&f);
    }
}
