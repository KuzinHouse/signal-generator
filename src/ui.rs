use crate::config::GeneratorConfig;
use crate::models::FlatEntry;

/// Русские названия каталогов + порядок отображения
pub const CATALOG_TITLES: &[(&str, &str)] = &[
    ("temperature", "Температура"),
    ("pressure", "Давление"),
    ("level", "Уровень"),
    ("flow", "Расход"),
    ("vibration", "Вибрация"),
    ("speed", "Скорость"),
    ("electrical", "Электрика"),
    ("water", "Вода / Водоподготовка"),
    ("climate", "Климат"),
    ("heating", "Теплоснабжение"),
    ("valve", "Клапаны / Состояния"),
    ("weight", "Вес"),
    ("modbus", "Modbus"),
    ("agronomy", "Агрономия"),
    ("livestock", "Животноводство"),
];

pub fn catalog_title(catalog: &str) -> String {
    CATALOG_TITLES
        .iter()
        .find(|(c, _)| *c == catalog)
        .map(|(_, t)| t.to_string())
        .unwrap_or_else(|| {
            if catalog.trim().is_empty() {
                "Без каталога".to_string()
            } else {
                catalog.to_string()
            }
        })
}

/// Сгенерировать HTML панелей генераторов с актуальными значениями (server-side).
/// Панели сгруппированы по каталогам (субтопикам).
pub fn render_panels_html(generators: &[GeneratorConfig], signals: &[Vec<FlatEntry>]) -> String {
    if generators.is_empty() {
        return "<div style=\"grid-column:1/-1;text-align:center;padding:30px;color:var(--t3);font-family:JetBrains Mono,monospace;font-size:10px;text-transform:uppercase\">Нет генераторов</div>".to_string();
    }
    // Группировка по каталогам: пустой каталог идёт последним
    let mut groups: Vec<(String, Vec<&GeneratorConfig>)> = Vec::new();
    for g in generators {
        let cat = if g.catalog.trim().is_empty() {
            String::new()
        } else {
            g.catalog.trim().trim_matches('/').to_string()
        };
        match groups.iter_mut().find(|(c, _)| *c == cat) {
            Some((_, v)) => v.push(g),
            None => groups.push((cat, vec![g])),
        }
    }
    // Сортировка: сначала по CATALOG_TITLES, потом без каталога
    groups.sort_by_key(|(cat, _)| {
        CATALOG_TITLES
            .iter()
            .position(|(c, _)| c == cat)
            .unwrap_or(usize::MAX)
    });
    let mut html = String::new();
    for (cat, group) in groups {
        html.push_str(&format!(
            "<div class=\"catalog-block\"><div class=\"catalog-header\"><span>{}</span><span class=\"catalog-topic\">USEPI/{}</span><span class=\"catalog-count\">{}</span></div>",
            html_escape(&catalog_title(&cat)),
            html_escape(&cat),
            group.len()
        ));
        for g in group {
            html.push_str(&panel_html(g, signals));
        }
        html.push_str("</div>");
    }
    html
}

fn panel_html(g: &GeneratorConfig, signals: &[Vec<FlatEntry>]) -> String {
    let wi = match g.wave_type {
        crate::config::WaveType::Sine => "∿",
        crate::config::WaveType::Sawtooth => "↗",
        crate::config::WaveType::Square => "⊓",
        crate::config::WaveType::Noise => "≈",
        crate::config::WaveType::Random => "?",
        crate::config::WaveType::Constant => "—",
    };
    // Найти последний сигнал генератора
    let (val_str, lat_str) = signal_value(g, signals);
    let cls = panel_class(g, signals);
    let corr_badge = g
        .correlation
        .as_ref()
        .map(|c| {
            format!(
                "<span class=\"corr-badge\">⇄ {}</span>",
                html_escape(&c.master_id)
            )
        })
        .unwrap_or_default();
    format!(
        "<div class=\"panel {}\" id=\"pn-{}\" data-edit=\"{}\"><div class=\"panel-header\"><span>{}</span><span>{}</span></div><div class=\"panel-value gradient-text {}\" id=\"pv-{}\">{}<small> {}</small></div><div class=\"panel-footer\">{} · {} {}ms{} · <span id=\"lt-{}\">{}</span>{}</div><div class=\"wave-icon\">{}</div><div class=\"panel-actions\" data-stop=\"1\"><label class=\"toggle\"><input type=\"checkbox\"{} data-toggle=\"{}\"><span class=\"toggle-slider\"></span></label><button class=\"pn-action\" data-del=\"{}\">✕</button></div></div>",
        if g.enabled { "" } else { "disabled" },
        g.id,
        g.id,
        html_escape(&g.name),
        html_escape(&g.id),
        cls,
        g.id,
        val_str,
        html_escape(&g.unit),
        html_escape(&g.topic),
        g.wave_type,
        g.interval_ms,
        if g.modbus_addr > 0 { format!(" · MB:{}", g.modbus_addr) } else { String::new() },
        g.id,
        lat_str,
        corr_badge,
        wi,
        if g.enabled { " checked" } else { "" },
        g.id,
        g.id,
    )
}

/// Извлечь значение и латентность из сигнала
fn signal_value(g: &GeneratorConfig, signals: &[Vec<FlatEntry>]) -> (String, String) {
    let full_id = format!("sensor/{}", g.id);
    let sig = signals
        .iter()
        .find(|p| p.iter().any(|e| e.id == full_id || e.id == g.id));
    let Some(sig) = sig else {
        return ("—".to_string(), "—".to_string());
    };
    let val = sig
        .iter()
        .find(|e| e.id == g.id)
        .and_then(|e| e.entry_value.as_f64());
    let ts = sig
        .iter()
        .find(|e| e.id == "timestamp")
        .and_then(|e| e.entry_value.as_str());
    let val_str = val
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "—".to_string());
    let lat_str = match ts {
        Some(t) => match chrono::DateTime::parse_from_rfc3339(t) {
            Ok(dt) => {
                let age_ms =
                    (chrono::Utc::now() - dt.with_timezone(&chrono::Utc)).num_milliseconds();
                if age_ms < 1000 {
                    format!("{}ms", age_ms)
                } else {
                    format!("{:.1}s", age_ms as f64 / 1000.0)
                }
            }
            Err(_) => "—".to_string(),
        },
        None => "—".to_string(),
    };
    (val_str, lat_str)
}

/// Класс панели по состоянию сигнала
fn panel_class(g: &GeneratorConfig, signals: &[Vec<FlatEntry>]) -> String {
    let full_id = format!("sensor/{}", g.id);
    let sig = signals
        .iter()
        .find(|p| p.iter().any(|e| e.id == full_id || e.id == g.id));
    let Some(sig) = sig else { return String::new() };
    let v = sig
        .iter()
        .find(|e| e.id == g.id)
        .and_then(|e| e.entry_value.as_f64());
    let q = sig
        .iter()
        .find(|e| e.id == format!("{}_quality", g.id))
        .and_then(|e| e.entry_value.as_u64());
    let Some(v) = v else { return String::new() };
    let q = q.unwrap_or(100);
    // Коррелированные сигналы центрируются вокруг master×factor+offset, а не offset —
    // нормализуем по середине диапазона, чтобы панели не алермили постоянно
    let (center, span) = if g.correlation.is_some() {
        ((g.min + g.max) * 0.5, (g.max - g.min).max(1.0) * 0.5)
    } else {
        (g.offset, g.amplitude.max(1.0))
    };
    let r = (v - center).abs() / span;
    if r > 0.9 || q < 50 {
        "alarm".to_string()
    } else if r > 0.7 || q < 80 {
        "warning".to_string()
    } else {
        String::new()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
