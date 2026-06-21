//! Minimal in-app localization. A thread-local "current language" (set from
//! Settings) drives `tr(key)` for UI-thread strings; worker threads that need a
//! translation (e.g. the dossier fetch) call `translate(lang, key)` explicitly,
//! since the thread-local isn't shared across threads.

use std::cell::Cell;

use serde::{Deserialize, Serialize};

use super::model::Kind;

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lang { En, Ru, Uk }

impl Default for Lang { fn default() -> Self { Lang::En } }

impl Lang {
    pub const ALL: [Lang; 3] = [Lang::En, Lang::Ru, Lang::Uk];

    /// Native name shown in the language picker.
    pub fn label(self) -> &'static str {
        match self { Lang::En => "English", Lang::Ru => "Русский", Lang::Uk => "Українська" }
    }

    /// Wikipedia subdomain / Wikidata label language code.
    pub fn code(self) -> &'static str {
        match self { Lang::En => "en", Lang::Ru => "ru", Lang::Uk => "uk" }
    }
}

thread_local! {
    static CUR: Cell<Lang> = const { Cell::new(Lang::En) };
}

pub fn set_lang(l: Lang) { CUR.with(|c| c.set(l)); }
pub fn lang() -> Lang { CUR.with(|c| c.get()) }

/// Translate a key for the current (thread-local) language.
pub fn tr(key: &str) -> &'static str { translate(lang(), key) }

/// Localized entity-kind label for the UI (the English `Kind::label()` is kept
/// for stable exports like Maltego `.mtgx` and CSV).
pub fn kind_label(k: Kind) -> &'static str {
    let l = lang();
    if l == Lang::En { return k.label(); }
    match k {
        Kind::Domain       => pick(l, "Domain", "Домен", "Домен"),
        Kind::Website      => pick(l, "Website", "Веб-сайт", "Веб-сайт"),
        Kind::Ip           => pick(l, "IP Address", "IP-адрес", "IP-адреса"),
        Kind::Email        => pick(l, "Email", "Эл. почта", "Ел. пошта"),
        Kind::Phone        => pick(l, "Phone", "Телефон", "Телефон"),
        Kind::Person       => pick(l, "Person", "Человек", "Людина"),
        Kind::Username     => pick(l, "Username", "Логин", "Логін"),
        Kind::Social       => pick(l, "Social Profile", "Соцпрофиль", "Соцпрофіль"),
        Kind::Organization => pick(l, "Organization", "Организация", "Організація"),
        Kind::Location     => pick(l, "Location", "Локация", "Локація"),
        Kind::Asn          => pick(l, "ASN", "ASN", "ASN"),
        Kind::Cve          => pick(l, "CVE", "CVE", "CVE"),
        Kind::BtcAddress   => pick(l, "BTC Address", "BTC-адрес", "BTC-адреса"),
        Kind::EthAddress   => pick(l, "ETH Address", "ETH-адрес", "ETH-адреса"),
        Kind::Transaction  => pick(l, "Transaction", "Транзакция", "Транзакція"),
        Kind::MacAddress   => pick(l, "MAC Address", "MAC-адрес", "MAC-адреса"),
        Kind::Coordinate   => pick(l, "Coordinate", "Координата", "Координата"),
        Kind::Document     => pick(l, "Document", "Документ", "Документ"),
        Kind::Service      => pick(l, "Service", "Сервис", "Сервіс"),
        Kind::OperatingSystem => pick(l, "OS", "ОС", "ОС"),
        Kind::File         => pick(l, "File", "Файл", "Файл"),
        Kind::Hash         => pick(l, "Hash", "Хеш", "Хеш"),
        Kind::Port         => pick(l, "Port", "Порт", "Порт"),
        Kind::Netblock     => pick(l, "Netblock", "Подсеть", "Підмережа"),
        Kind::Phrase       => pick(l, "Phrase", "Заметка", "Нотатка"),
    }
}

fn pick(lang: Lang, en: &'static str, ru: &'static str, uk: &'static str) -> &'static str {
    match lang { Lang::En => en, Lang::Ru => ru, Lang::Uk => uk }
}

/// Translate a key for an explicit language (usable off the UI thread).
pub fn translate(lang: Lang, key: &str) -> &'static str {
    match key {
        // ── tabs / shell ────────────────────────────────────────────────────
        "tab.graph"    => pick(lang, "Graph", "Граф", "Граф"),
        "tab.geo"      => pick(lang, "GEOINT", "ГЕОИНТ", "ГЕОІНТ"),
        "tab.monitor"  => pick(lang, "Monitor", "Монитор", "Монітор"),
        "tab.dossier"  => pick(lang, "Dossier", "Досье", "Досьє"),
        "tab.cases"    => pick(lang, "Cases", "Дела", "Справи"),
        "tab.watch"    => pick(lang, "Watch", "Вотч", "Вотч"),
        "tab.toolbox"  => pick(lang, "Toolbox", "Утилиты", "Утиліти"),
        "shell.settings" => pick(lang, "Settings", "Настройки", "Налаштування"),
        "shell.help"   => pick(lang, "Help", "Помощь", "Довідка"),

        // ── settings ────────────────────────────────────────────────────────
        "set.language" => pick(lang, "LANGUAGE", "ЯЗЫК", "МОВА"),
        "set.interface"=> pick(lang, "INTERFACE", "ИНТЕРФЕЙС", "ІНТЕРФЕЙС"),
        "set.theme"    => pick(lang, "THEME", "ТЕМА", "ТЕМА"),
        "set.accent"   => pick(lang, "ACCENT COLOUR", "ЦВЕТ АКЦЕНТА", "КОЛІР АКЦЕНТУ"),
        "set.canvas"   => pick(lang, "CANVAS", "ХОЛСТ", "ПОЛОТНО"),

        // ── dossier UI ──────────────────────────────────────────────────────
        "dos.subject"  => pick(lang, "Subject", "Цель", "Ціль"),
        "dos.subject_hint" => pick(lang, "person, organization, place…",
                                         "человек, организация, место…",
                                         "людина, організація, місце…"),
        "dos.build"    => pick(lang, "Build dossier", "Собрать досье", "Зібрати досьє"),
        "dos.custom_db"=> pick(lang, "Custom DB", "Своя база", "Своя база"),
        "dos.load"     => pick(lang, "Load", "Загрузить", "Завантажити"),
        "dos.db_path_hint" => pick(lang, "path to a .json database",
                                         "путь к базе .json", "шлях до бази .json"),
        "dos.no_db"    => pick(lang, "no custom DB — using Wikipedia + Wikidata",
                                     "без своей базы — Wikipedia + Wikidata",
                                     "без своєї бази — Wikipedia + Wikidata"),
        "dos.title"    => pick(lang, "Dossier", "Досье", "Досьє"),
        "dos.tagline1" => pick(lang, "Type a subject — parasite assembles a profile from Wikipedia,",
                                     "Введите цель — паразит соберёт профиль из Wikipedia,",
                                     "Введіть ціль — паразит збере профіль з Wikipedia,"),
        "dos.tagline2" => pick(lang, "Wikidata and your own database, and lays it out as a report.",
                                     "Wikidata и вашей базы, и оформит как отчёт.",
                                     "Wikidata і вашої бази, та оформить як звіт."),
        "dos.try"      => pick(lang, "TRY ONE", "ПОПРОБУЙТЕ", "СПРОБУЙТЕ"),
        "dos.warn"     => pick(lang, "experimental — open-source intel only, verify before relying on it",
                                     "экспериментально — только открытые данные, перепроверяйте",
                                     "експериментально — лише відкриті дані, перевіряйте"),
        "dos.reading"  => pick(lang, "reading the sources", "читаю источники", "читаю джерела"),
        "dos.not_found"=> pick(lang, "Nothing found", "Ничего не найдено", "Нічого не знайдено"),
        "dos.not_found_hint" => pick(lang, "No source had a record for this subject. Try a fuller or different spelling.",
                                          "Ни в одном источнике нет записи. Попробуйте полнее или иначе.",
                                          "У жодному джерелі немає запису. Спробуйте повніше або інакше."),
        "dos.retry"    => pick(lang, "Try again", "Повторить", "Повторити"),

        // ── dossier sections & facts ────────────────────────────────────────
        "sec.summary"  => pick(lang, "Summary", "Сводка", "Зведення"),
        "sec.identity" => pick(lang, "Identity", "Личность", "Особа"),
        "sec.career"   => pick(lang, "Career", "Карьера", "Кар'єра"),
        "sec.personal" => pick(lang, "Personal", "Личное", "Особисте"),
        "sec.links"    => pick(lang, "Links & online presence", "Ссылки и онлайн-присутствие",
                                     "Посилання та онлайн-присутність"),
        "sec.more"     => pick(lang, "More sources", "Другие источники", "Інші джерела"),
        "sec.custom"   => pick(lang, "Custom database", "Своя база", "Своя база"),

        "f.gender"     => pick(lang, "Gender", "Пол", "Стать"),
        "f.born"       => pick(lang, "Born", "Родился", "Народився"),
        "f.birthplace" => pick(lang, "Birthplace", "Место рождения", "Місце народження"),
        "f.citizenship"=> pick(lang, "Citizenship", "Гражданство", "Громадянство"),
        "f.died"       => pick(lang, "Died", "Умер", "Помер"),
        "f.death_place"=> pick(lang, "Place of death", "Место смерти", "Місце смерті"),
        "f.occupation" => pick(lang, "Occupation", "Род занятий", "Рід занять"),
        "f.employer"   => pick(lang, "Employer", "Работодатель", "Роботодавець"),
        "f.education"  => pick(lang, "Education", "Образование", "Освіта"),
        "f.position"   => pick(lang, "Position", "Должность", "Посада"),
        "f.awards"     => pick(lang, "Awards", "Награды", "Нагороди"),
        "f.spouse"     => pick(lang, "Spouse", "Супруг(а)", "Дружина/чоловік"),
        "f.field"      => pick(lang, "Field of work", "Сфера деятельности", "Сфера діяльності"),
        "f.religion"   => pick(lang, "Religion", "Религия", "Релігія"),

        "l.official"   => pick(lang, "Official site", "Официальный сайт", "Офіційний сайт"),
        "l.web_search" => pick(lang, "Web search", "Поиск в вебе", "Пошук у вебі"),

        // ── dossier → graph ─────────────────────────────────────────────────
        "dos.sync_graph" => pick(lang, "auto → graph", "авто → граф", "авто → граф"),
        "dos.to_graph" => pick(lang, "Add to graph", "В граф", "До графу"),
        "dos.added_graph" => pick(lang, "Added to graph ✓", "Добавлено в граф ✓", "Додано до графу ✓"),
        "dos.ai"        => pick(lang, "AI assess", "ИИ-оценка", "ШІ-оцінка"),
        "dos.ai_working"=> pick(lang, "thinking…", "думаю…", "думаю…"),
        "dos.ai_title"  => pick(lang, "AI assessment", "Оценка ИИ", "Оцінка ШІ"),
        "dos.chat"      => pick(lang, "Chat", "Чат", "Чат"),
        "dos.chat_title"=> pick(lang, "AI chat", "ИИ-чат", "ШІ-чат"),
        "dos.chat_ph"   => pick(lang, "ask about the subject…", "спроси о цели…", "запитай про ціль…"),
        "dos.chat_sites"=> pick(lang, "fetch & analyze the source sites", "загрузить и анализировать сайты-источники",
                                     "завантажити й аналізувати сайти-джерела"),
        "dos.chat_hello"=> pick(lang, "Ask anything about this subject. Hit ⊕ to pull in the source pages so I can analyze them.",
                                     "Спроси что угодно о цели. Нажми ⊕, чтобы подтянуть сайты для анализа.",
                                     "Запитай будь-що про ціль. Натисни ⊕, щоб підтягнути сайти для аналізу."),

        // ── pentest mode ────────────────────────────────────────────────────
        "tab.pentest"  => pick(lang, "Pentest", "Пентест", "Пентест"),
        "pt.target"    => pick(lang, "Target", "Цель", "Ціль"),
        "pt.target_hint" => pick(lang, "IP, host or URL", "IP, хост или URL", "IP, хост або URL"),
        "pt.tagline"   => pick(lang, "Pick a phase, set the target — commands are built for you. Copy, or send to the graph.",
                                     "Выбери фазу, задай цель — команды соберутся сами. Копируй или отправь в граф.",
                                     "Обери фазу, задай ціль — команди зберуться самі. Копіюй або надішли в граф."),
        "pt.arsenal"   => pick(lang, "Arsenal", "Арсенал", "Арсенал"),
        "pt.install_all" => pick(lang, "install all", "ставить все", "ставити всі"),
        "pt.install_arsenal" => pick(lang, "Install full arsenal", "Поставить весь арсенал", "Поставити весь арсенал"),
        "pt.copy"      => pick(lang, "Copy", "Копировать", "Копіювати"),
        "pt.copied"    => pick(lang, "Copied to clipboard ✓", "Скопировано ✓", "Скопійовано ✓"),
        "pt.to_graph"  => pick(lang, "→ Graph", "→ Граф", "→ Граф"),
        "pt.set_target"=> pick(lang, "set a target above", "укажите цель выше", "вкажіть ціль вище"),
        "pt.ph_recon"  => pick(lang, "Recon & scanning", "Разведка и сканы", "Розвідка та скани"),
        "pt.ph_web"    => pick(lang, "Web & content", "Веб и контент", "Веб і контент"),
        "pt.ph_creds"  => pick(lang, "Credentials & brute", "Учётки и брут", "Облікові та брут"),
        "pt.ph_exploit"=> pick(lang, "Exploitation", "Эксплуатация", "Експлуатація"),
        "pt.ph_post"   => pick(lang, "Post-exploitation", "Пост-эксплуатация", "Пост-експлуатація"),
        "pt.ph_listen" => pick(lang, "Shells & listeners", "Шеллы и листенеры", "Шели та слухачі"),

        // ── graph: toolbar ──────────────────────────────────────────────────
        "gr.new"       => pick(lang, "New", "Новый", "Новий"),
        "gr.open"      => pick(lang, "Open", "Открыть", "Відкрити"),
        "gr.save"      => pick(lang, "Save", "Сохранить", "Зберегти"),
        "gr.layout"    => pick(lang, "Layout", "Раскладка", "Розкладка"),
        "gr.fit"       => pick(lang, "Fit", "Вместить", "Вмістити"),
        "gr.table"     => pick(lang, "Table", "Таблица", "Таблиця"),
        "gr.analytics" => pick(lang, "Analytics", "Аналитика", "Аналітика"),
        "gr.video"     => pick(lang, "Video", "Видео", "Відео"),
        "gr.clear"     => pick(lang, "Clear", "Очистить", "Очистити"),

        // ── graph: palette / details ────────────────────────────────────────
        "gr.entities"  => pick(lang, "ENTITIES", "СУЩНОСТИ", "СУТНОСТІ"),
        "gr.add"       => pick(lang, "Add", "Добавить", "Додати"),
        "gr.search"    => pick(lang, "search…", "поиск…", "пошук…"),
        "gr.value"     => pick(lang, "VALUE", "ЗНАЧЕНИЕ", "ЗНАЧЕННЯ"),
        "gr.flag"      => pick(lang, "FLAG", "МЕТКА", "ПОЗНАЧКА"),
        "gr.note"      => pick(lang, "NOTE", "ЗАМЕТКА", "НОТАТКА"),
        "gr.props"     => pick(lang, "PROPERTIES", "СВОЙСТВА", "ВЛАСТИВОСТІ"),
        "gr.image"     => pick(lang, "IMAGE", "ИЗОБРАЖЕНИЕ", "ЗОБРАЖЕННЯ"),
        "gr.set_image" => pick(lang, "set image", "задать", "задати"),
        "gr.upload_image" => pick(lang, "Upload image", "Загрузить фото", "Завантажити фото"),
        "gr.change_image" => pick(lang, "Change image", "Заменить фото", "Замінити фото"),
        "gr.clear_image" => pick(lang, "clear", "убрать", "прибрати"),
        "gr.open_browser" => pick(lang, "Open in browser", "Открыть в браузере", "Відкрити в браузері"),
        "gr.no_sel"    => pick(lang, "No entity selected", "Сущность не выбрана", "Сутність не вибрана"),
        "gr.machines"  => pick(lang, "MACHINES", "МАШИНЫ", "МАШИНИ"),

        // ── graph: AI builder ───────────────────────────────────────────────
        "gr.ai"        => pick(lang, "AI", "ИИ", "ШІ"),
        "gr.ai_title"  => pick(lang, "AI graph builder", "ИИ-сборка графа", "ШІ-збірка графу"),
        "gr.ai_hint"   => pick(lang, "Describe a target — the AI designs entities & links to investigate.",
                                     "Опиши цель — ИИ построит сущности и связи для проверки.",
                                     "Опиши ціль — ШІ побудує сутності та зв'язки для перевірки."),
        "gr.ai_ph"     => pick(lang, "e.g. investigate the company Tesla and its key people",
                                     "напр. пробей компанию Tesla и её ключевых людей",
                                     "напр. пробий компанію Tesla та її ключових людей"),
        "gr.ai_build"  => pick(lang, "Build graph", "Построить граф", "Побудувати граф"),
        "gr.ai_expand" => pick(lang, "Expand current", "Расширить текущий", "Розширити поточний"),
        "gr.ai_working"=> pick(lang, "thinking…", "думаю…", "думаю…"),
        "gr.ai_using"  => pick(lang, "using", "через", "через"),
        "gr.ai_nokey"  => pick(lang, "no AI key — add Claude or Gemini in Settings → API keys",
                                     "нет ключа ИИ — добавь Claude или Gemini в Настройки → API keys",
                                     "немає ключа ШІ — додай Claude або Gemini в Налаштування → API keys"),
        "gr.ai_warn"   => pick(lang, "⚠ AI output is a hypothesis — verify with real transforms.",
                                     "⚠ Вывод ИИ — гипотеза, проверяй трансформами.",
                                     "⚠ Вивід ШІ — гіпотеза, перевіряй трансформами."),
        "gr.intel"     => pick(lang, "Intel", "Интел", "Інтел"),
        "gr.advisor"   => pick(lang, "Insights", "Инсайты", "Інсайти"),
        "gr.inst_triage" => pick(lang, "Auto-triage & flag", "Авто-триаж и флажки", "Авто-тріаж і прапорці"),
        "gr.inst_triage_hint" => pick(lang,
            "Check IPs (GreyNoise) and hosts (HTTP) and flag them — red=bad, green=ok, orange=noisy. No AI.",
            "Проверяет IP (GreyNoise) и хосты (HTTP) и ставит флажки — красный=плохо, зелёный=ок, оранжевый=шум. Без ИИ.",
            "Перевіряє IP (GreyNoise) і хости (HTTP) і ставить прапорці — червоний=погано, зелений=ок, помаранчевий=шум. Без ШІ."),
        "gr.inst_notargets" => pick(lang, "Instinct: no IPs/domains to triage",
            "Инстинкт: нет IP/доменов для триажа", "Інстинкт: немає IP/доменів для тріажу"),
        "gr.inst_sub"    => pick(lang, "rule brain · no AI", "мозг на правилах · без ИИ", "мозок на правилах · без ШІ"),
        "gr.inst_rules"  => pick(lang, "RULES (Lisp) — applied live", "ПРАВИЛА (Lisp) — применяются вживую", "ПРАВИЛА (Lisp) — застосовуються наживо"),
        "gr.inst_hints"  => pick(lang, "hints", "подсказок", "підказок"),
        "gr.coverage"    => pick(lang, "Coverage", "Покрытие", "Покриття"),
        "gr.cov_hint"    => pick(lang, "Green = check already run · click ▷ to run a missing one.",
            "Зелёный = проверка уже выполнена · нажми ▷ чтобы запустить недостающую.",
            "Зелений = перевірку вже виконано · натисни ▷ щоб запустити відсутню."),
        "gr.cov_empty"   => pick(lang, "No entities yet.", "Пока нет сущностей.", "Поки немає сутностей."),
        "gr.cov_runall"  => pick(lang, "Run all missing checks", "Прогнать все недостающие", "Прогнати всі відсутні"),
        "gr.cov_runall_hint" => pick(lang, "run every built-in check not yet run, across the whole graph",
            "запустить все встроенные проверки, которых ещё не было, по всему графу",
            "запустити всі вбудовані перевірки, яких ще не було, по всьому графу"),
        "gr.report"      => pick(lang, "Report", "Отчёт", "Звіт"),
        "gr.cmd"         => pick(lang, "Command", "Команда", "Команда"),
        "gr.cmd_ph"      => pick(lang, "run a transform or machine on the selected node…",
            "запусти трансформ или машину на выбранном узле…", "запусти трансформ або машину на вибраному вузлі…"),
        "gr.cmd_nonode"  => pick(lang, "Select a node first.", "Сначала выберите узел.", "Спершу виберіть вузол."),
        "gr.inst_why"    => pick(lang, "the facts that made this rule fire",
            "факты, из-за которых сработало правило", "факти, через які спрацювало правило"),
        "gr.inst_export" => pick(lang, "Export pack", "Экспорт набора", "Експорт набору"),
        "gr.inst_import" => pick(lang, "Import pack", "Импорт набора", "Імпорт набору"),
        "pal.infra"    => pick(lang, "Infrastructure", "Инфраструктура", "Інфраструктура"),
        "pal.personal" => pick(lang, "Personal", "Личное", "Особисте"),
        "pal.locations"=> pick(lang, "Locations", "Локации", "Локації"),
        "pal.malware"  => pick(lang, "Malware & Files", "Вредонос и файлы", "Шкідливе та файли"),
        "pal.crypto"   => pick(lang, "Cryptocurrency", "Криптовалюта", "Криптовалюта"),
        "pal.other"    => pick(lang, "Other", "Прочее", "Інше"),
        "gr.filter"    => pick(lang, "filter…", "фильтр…", "фільтр…"),
        "gr.help"      => pick(lang, "Keyboard shortcuts", "Горячие клавиши", "Гарячі клавіші"),
        "gr.minimap"   => pick(lang, "Minimap", "Миникарта", "Мінімапа"),
        "gr.link"      => pick(lang, "Link", "Связать", "Зв'язати"),
        "gr.unlink"    => pick(lang, "Unlink", "Разорвать", "Розірвати"),
        "gr.lay_force"  => pick(lang, "Force-directed (organic)", "Силовая (органика)", "Силова (органіка)"),
        "gr.lay_tree"   => pick(lang, "Tree / hierarchical", "Дерево / иерархия", "Дерево / ієрархія"),
        "gr.lay_radial" => pick(lang, "Radial / concentric", "Радиальная / кольца", "Радіальна / кільця"),
        "gr.lay_circle" => pick(lang, "Circle", "Круг", "Коло"),
        "gr.lay_spiral" => pick(lang, "Spiral", "Спираль", "Спіраль"),
        "gr.lay_grid"   => pick(lang, "Grid", "Сетка", "Сітка"),
        "gr.lay_cols"   => pick(lang, "Columns by type (block)", "Колонки по типу (блок)", "Колонки за типом (блок)"),
        "gr.lay_scatter"=> pick(lang, "Scatter", "Разброс", "Розкид"),
        "gr.undo"       => pick(lang, "Undo (Ctrl+Z)", "Отменить (Ctrl+Z)", "Скасувати (Ctrl+Z)"),
        "gr.redo"       => pick(lang, "Redo (Ctrl+Y)", "Повторить (Ctrl+Y)", "Повторити (Ctrl+Y)"),
        "gr.advisor_none"  => pick(lang, "Nothing to suggest yet — add entities and expand them.",
                                         "Пока нечего предложить — добавьте сущности и раскройте их.",
                                         "Поки нема що запропонувати — додайте сутності та розкрийте їх."),
        "gr.intel_title" => pick(lang, "Quick Intel", "Быстрый интел", "Швидкий інтел"),
        "gr.intel_hint" => pick(lang, "Paste a phone or email — it lands on the graph and runs the lookups.",
                                      "Вставь телефон или почту — попадёт на граф и прогонит проверки.",
                                      "Встав телефон або пошту — потрапить на граф і прожене перевірки."),
        "gr.intel_phone" => pick(lang, "Phone (HLR)", "Телефон (HLR)", "Телефон (HLR)"),
        "gr.intel_email" => pick(lang, "Email (regs)", "Почта (рег)", "Пошта (рег)"),
        "gr.intel_note" => pick(lang, "phone → carrier/line-type + search; email → registrations (holehe) + breaches",
                                      "телефон → оператор/тип + поиск; почта → регистрации (holehe) + утечки",
                                      "телефон → оператор/тип + пошук; пошта → реєстрації (holehe) + витоки"),
        // ── Cases ──
        "cs.name"      => pick(lang, "name", "имя", "ім'я"),
        "cs.save"      => pick(lang, "▲ Save current graph", "▲ Сохранить текущий граф", "▲ Зберегти поточний граф"),
        "cs.open"      => pick(lang, "▼ Open", "▼ Открыть", "▼ Відкрити"),
        "cs.empty"     => pick(lang, "No saved cases yet", "Пока нет сохранённых дел", "Поки немає збережених справ"),
        "cs.empty_hint"=> pick(lang, "Build a graph, then save it here as a named case. Open any case to load it back into the Graph tab.",
                                     "Постройте граф и сохраните его здесь как именованное дело. Откройте дело, чтобы вернуть его в граф.",
                                     "Побудуйте граф і збережіть його тут як іменовану справу. Відкрийте справу, щоб повернути її у граф."),
        "cs.note_ph"   => pick(lang, "notes for this case (session-only)…", "заметки по делу (на сессию)…", "нотатки по справі (на сесію)…"),
        // ── Watch ──
        "wt.add"       => pick(lang, "＋ Add", "＋ Добавить", "＋ Додати"),
        "wt.checkall"  => pick(lang, "⟳ Check all", "⟳ Проверить все", "⟳ Перевірити все"),
        "wt.auto"      => pick(lang, "auto (5 min)", "авто (5 мин)", "авто (5 хв)"),
        "wt.value"     => pick(lang, "value to watch", "что отслеживать", "що відстежувати"),
        "wt.alerts"    => pick(lang, "ALERTS", "ОПОВЕЩЕНИЯ", "СПОВІЩЕННЯ"),
        "wt.clear"     => pick(lang, "clear", "очистить", "очистити"),
        "wt.nochanges" => pick(lang, "no changes detected yet", "изменений пока нет", "змін поки немає"),
        "wt.empty"     => pick(lang, "Watch a target for changes", "Следить за целью", "Стежити за ціллю"),
        "wt.empty_hint"=> pick(lang, "Domains (new certs/subdomains), GitHub users (activity) or BTC addresses (new tx). You get an alert when anything moves.",
                                     "Домены (новые сертификаты/субдомены), GitHub-юзеры (активность) или BTC-адреса (новые транзакции). Оповещение при любом изменении.",
                                     "Домени (нові сертифікати/субдомени), GitHub-юзери (активність) або BTC-адреси (нові транзакції). Сповіщення за будь-якої зміни."),
        "wt.try"       => pick(lang, "TRY ONE", "ПОПРОБУЙ", "СПРОБУЙ"),
        // ── Toolbox ──
        "tb.input"     => pick(lang, "INPUT", "ВВОД", "ВВЕДЕННЯ"),
        "tb.output"    => pick(lang, "OUTPUT", "ВЫВОД", "ВИВІД"),
        "tb.copy"      => pick(lang, "▣ copy", "▣ копировать", "▣ копіювати"),
        "gr.chat"      => pick(lang, "Chat", "Чат", "Чат"),
        "gr.chat_title"=> pick(lang, "Graph assistant", "Ассистент графа", "Асистент графу"),
        "gr.chat_ph"   => pick(lang, "ask about the graph…", "спроси о графе…", "запитай про граф…"),
        "gr.chat_plan" => pick(lang, "Agent: plan next steps & highlight nodes",
                                     "Агент: спланировать шаги и подсветить узлы",
                                     "Агент: спланувати кроки та підсвітити вузли"),
        "gr.chat_hello"=> pick(lang, "Ask about your graph, or hit ◉ to have the agent plan the next steps and highlight which nodes to expand.",
                                     "Спроси про граф, или жми ◉ — агент спланирует следующие шаги и подсветит узлы.",
                                     "Запитай про граф, або тисни ◉ — агент спланує наступні кроки та підсвітить вузли."),

        // ── welcome ─────────────────────────────────────────────────────────
        "wel.tagline"  => pick(lang, "an open-source, graph-based OSINT toolkit — a free Maltego alternative",
                                     "открытый OSINT-инструмент на графах — бесплатная замена Maltego",
                                     "відкритий OSINT-інструмент на графах — безкоштовна заміна Maltego"),
        "wel.try"      => pick(lang, "TRY IT", "ПОПРОБУЙТЕ", "СПРОБУЙТЕ"),
        "wel.try_hint" => pick(lang, "Add a Username \"torvalds\" → right-click → Hunt Accounts",
                                     "Добавь Username «torvalds» → ПКМ → Hunt Accounts",
                                     "Додай Username «torvalds» → ПКМ → Hunt Accounts"),
        "wel.start"    => pick(lang, "Get started", "Начать", "Почати"),
        "wel.warn"     => pick(lang, "for authorized security testing & research only",
                                     "только для авторизованного тестирования и исследований",
                                     "лише для авторизованого тестування та досліджень"),

        _ => "",
    }
}
