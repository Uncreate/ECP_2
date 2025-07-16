use eframe::{egui, egui::widgets::Separator, App, Frame, NativeOptions};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::HashSet, fs, path::PathBuf};

const ONLINE_DB: &str = "https://media.githubusercontent.com/media/Uncreate/EssaiControlPanel/refs/heads/main/MasterToolDatabase.txt";

// ---------- Data loading ----------
#[derive(Deserialize)]
struct ToolDatabase { tools: Vec<ToolEntry> }

#[derive(Deserialize)]
struct ToolEntry {
    tool_name: String,
    sc_tool_type: String,
    #[serde(default, rename = "Solfex")] solfex: Option<Value>,
    #[serde(default)] milling_tool: Option<Value>,
    #[serde(default)] drilling_tool: Option<Value>,
}

fn val_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn field(sec: &Option<Value>, key: &str) -> Option<String> { sec.as_ref()?.get(key).map(val_to_string) }

#[derive(Clone)]
struct ToolItem {
    tool_name: String,
    essai_part: String,
    edp_num: String,
    manufacturer: String,
    holder_name: String,
    outside_len: String,
    gage_len: String,
    description: String,
    diameter: String,
    loc: String,
    solfex: Option<Value>,
    milling: Option<Value>,
    drilling: Option<Value>,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum DBSource { Local, Online }

fn local_db() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("MasterToolDatabase.txt") }

fn fetch_db(src: DBSource) -> String {
    match src {
        DBSource::Local => fs::read_to_string(local_db()).unwrap_or_default(),
        DBSource::Online => reqwest::blocking::get(ONLINE_DB).and_then(|r| r.text()).unwrap_or_default(),
    }
}

fn parse_items(raw: &str) -> Vec<ToolItem> {
    let db: ToolDatabase = serde_json::from_str(raw).unwrap_or(ToolDatabase { tools: vec![] });
    db.tools
        .into_iter()
        .map(|t| {
            let (pri, sec) = match t.sc_tool_type.as_str() {
                "drilling" => (&t.drilling_tool, &t.milling_tool),
                _ => (&t.milling_tool, &t.drilling_tool),
            };
            let pick = |k: &str| field(pri, k).or_else(|| field(sec, k)).unwrap_or_default();
            ToolItem {
                tool_name: t.tool_name,
                essai_part: pick("Message2"),
                edp_num: pick("Message1"),
                manufacturer: pick("Message3"),
                holder_name: pick("HolderName"),
                outside_len: pick("Length"),
                gage_len: pick("HLength"),
                description: pick("Description"),
                diameter: pick("Diameter"),
                loc: pick("CuttingLength"),
                solfex: t.solfex,
                milling: t.milling_tool,
                drilling: t.drilling_tool,
            }
        })
        .collect()
}

// ---------- UI State ----------
#[derive(Default, PartialEq, Eq, Clone, Copy)]
enum ActiveTab { #[default] Solfex, Milling, Drilling }
#[derive(Clone)]
enum ToolFilter { Family(String), Class(String, String) }

struct MyApp {
    source: DBSource,
    items: Vec<ToolItem>,
    manufacturers: Vec<String>,
    search: String,
    manufacturer_filter: Option<String>,
    tool_filter: Option<ToolFilter>,
    selected: Option<usize>,
    active_tab: ActiveTab,
    solfex_keys: Vec<String>,
    milling_keys: Vec<String>,
    drilling_keys: Vec<String>,
    show_local_warning: bool,
}

impl Default for MyApp {
    fn default() -> Self {
        let source = DBSource::Online; // default to ONLINE
        let items = parse_items(&fetch_db(source));
        let manufacturers = unique(items.iter().map(|t| &t.manufacturer));
        let solf = collect_keys(&items, |t| &t.solfex);
        let mil = collect_keys(&items, |t| &t.milling);
        let dri = collect_keys(&items, |t| &t.drilling);
        Self { source, items, manufacturers, search: String::new(), manufacturer_filter: None, tool_filter: None, selected: None, active_tab: ActiveTab::Solfex, solfex_keys: solf, milling_keys: mil, drilling_keys: dri, show_local_warning: false }
    }
}

fn unique<'a, I: IntoIterator<Item = &'a String>>(iter: I) -> Vec<String> {
    let mut v: Vec<_> = iter.into_iter().filter(|s| !s.is_empty()).cloned().collect();
    v.sort(); v.dedup(); v
}

fn collect_keys<F>(items: &[ToolItem], sel: F) -> Vec<String> where F: Fn(&ToolItem) -> &Option<Value> {
    let mut set = HashSet::new();
    for it in items { if let Some(Value::Object(map)) = sel(it) { set.extend(map.keys().cloned()); } }
    let mut v: Vec<_> = set.into_iter().collect(); v.sort(); v
}

impl MyApp {
    fn reload(&mut self) {
        self.items = parse_items(&fetch_db(self.source));
        self.manufacturers = unique(self.items.iter().map(|t| &t.manufacturer));
        self.solfex_keys = collect_keys(&self.items, |t| &t.solfex);
        self.milling_keys = collect_keys(&self.items, |t| &t.milling);
        self.drilling_keys = collect_keys(&self.items, |t| &t.drilling);
        self.selected = None;
    }

    fn passes(&self, it: &ToolItem) -> bool {
        if let Some(m) = &self.manufacturer_filter { if &it.manufacturer != m { return false; } }
        if let Some(f) = &self.tool_filter { return match f { ToolFilter::Family(e) => &it.essai_part == e, ToolFilter::Class(e,h) => &it.essai_part == e && &it.holder_name == h } }
        self.search.is_empty() || it.tool_name.to_lowercase().contains(&self.search.to_lowercase())
    }

    fn chip(&self) -> Option<String> {
        self.tool_filter.as_ref().map(|f| match f { ToolFilter::Family(m) => format!("Family: {m}"), ToolFilter::Class(m,h) => format!("Class: {m} | {h}") })
    }
}

// ---------- App Impl ----------
impl App for MyApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        // Top bar
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            ui.horizontal(|ui| {
                // DB toggle
                let prev = self.source;
                ui.menu_button("Database", |ui| {
                    if ui.radio_value(&mut self.source, DBSource::Local, "Local").clicked() { self.show_local_warning = true; }
                    ui.radio_value(&mut self.source, DBSource::Online, "Online");
                });
                if prev != self.source { self.reload(); }

                // Manufacturer filter
                ui.menu_button("Manufacturer", |ui| {
                    if ui.button("All").clicked() { self.manufacturer_filter = None; ui.close(); }
                    for m in &self.manufacturers { if ui.button(m).clicked() { self.manufacturer_filter = Some(m.clone()); ui.close(); } }
                });
                if let Some(m) = &self.manufacturer_filter { ui.label(format!("Manufacturer: {m}")); if ui.small_button("✖").clicked() { self.manufacturer_filter = None; } }
            });
        });

        // LOCAL warning modal
        if self.show_local_warning {
            egui::Window::new("Warning")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.colored_label(egui::Color32::BLACK, "⚠️ You are using the LOCAL database. Data may be outdated. Use only for testing.");
                    ui.add_space(10.0);
                    if ui.button("OK").clicked() { self.show_local_warning = false; }
                });
        }

        // Left panel
        egui::SidePanel::left("left").min_width(220.0).show(ctx, |ui| {
            ui.heading("Tool List"); ui.add_space(4.0);
            ui.horizontal(|ui| { ui.label("🔍"); ui.text_edit_singleline(&mut self.search); });
            if let Some(ch) = self.chip() { ui.add_space(4.0); ui.horizontal(|ui| { ui.label(ch); if ui.small_button("✖").clicked() { self.tool_filter = None; } }); }
            ui.add(Separator::default());
            let mut next_sel = self.selected; let mut next_filter = None;
            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                for (i, it) in self.items.iter().enumerate() {
                    if !self.passes(it) { continue; }
                    let sel = self.selected == Some(i);
                    let resp = ui.selectable_label(sel, &it.tool_name);
                    if resp.clicked() { next_sel = if sel { None } else { Some(i) }; }
                    resp.context_menu(|ui| {
                        if ui.button("Show Tool Family").clicked() { if !it.essai_part.is_empty() { next_filter = Some(ToolFilter::Family(it.essai_part.clone())); } ui.close(); }
                        if ui.button("Show Tool Class").clicked() { if !it.essai_part.is_empty() { next_filter = Some(ToolFilter::Class(it.essai_part.clone(), it.holder_name.clone())); } ui.close(); }
                    });
                }
            });
            ui.add_space(12.0);
            if ui.button("🔄 Refresh").clicked() {
                self.reload();
            }
            self.selected = next_sel; if let Some(f) = next_filter { self.tool_filter = Some(f); }
        });

        // Right panel
        egui::SidePanel::right("right").min_width(280.0).show(ctx, |ui| {
            ui.heading("Assembly Details"); ui.add_space(4.0);
            if let Some(idx) = self.selected {
                let it = &self.items[idx];
                egui::Grid::new("asm").striped(true).show(ui, |ui| {
                    ui.label("Tool Name"); ui.label(&it.tool_name); ui.end_row();
                    ui.label("Holder"); ui.label(&it.holder_name); ui.end_row();
                    ui.label("Outside Holder L"); ui.label(&it.outside_len); ui.end_row();
                    ui.label("Gage Length"); ui.label(&it.gage_len); ui.end_row();
                });
                ui.add_space(6.0);
                ui.heading("Tool Details"); ui.add_space(4.0);
                egui::Grid::new("td").striped(true).show(ui, |ui| {
                    ui.label("Essai Part #"); ui.label(&it.essai_part); ui.end_row();
                    ui.label("Manufacturer"); ui.label(&it.manufacturer); ui.end_row();
                    ui.label("EDP #"); ui.label(&it.edp_num); ui.end_row();
                });
                ui.add_space(6.0);
                ui.heading("Tool Details Extended"); ui.add_space(4.0);
                egui::Grid::new("tdx").striped(true).show(ui, |ui| {
                    ui.label("Description"); ui.label(&it.description); ui.end_row();
                    ui.label("Diameter"); ui.label(&it.diameter); ui.end_row();
                    ui.label("Length of Cut"); ui.label(&it.loc); ui.end_row();
                });
            } else {
                ui.label("Select a tool from the list");
            }
        });

        // Center panel
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (tab, lbl) in [
                    (ActiveTab::Solfex, "Solfex"),
                    (ActiveTab::Milling, "Milling"),
                    (ActiveTab::Drilling, "Drilling"),
                ] {
                    if ui.selectable_label(self.active_tab == tab, lbl).clicked() {
                        self.active_tab = tab;
                    }
                }
            });
            ui.add(Separator::default());
            let (section, keys) = match self.active_tab {
                ActiveTab::Solfex => (
                    self.selected.and_then(|i| self.items[i].solfex.as_ref()),
                    &self.solfex_keys,
                ),
                ActiveTab::Milling => (
                    self.selected.and_then(|i| self.items[i].milling.as_ref()),
                    &self.milling_keys,
                ),
                ActiveTab::Drilling => (
                    self.selected.and_then(|i| self.items[i].drilling.as_ref()),
                    &self.drilling_keys,
                ),
            };
            json_table(ui, section, keys);
        });
    }
}

// ---------- Helpers ----------
fn json_table(ui: &mut egui::Ui, section: Option<&Value>, keys: &[String]) {
    let map = section.and_then(|v| v.as_object());
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("json_table").striped(true).show(ui, |ui| {
            for k in keys {
                let val = map
                    .and_then(|m| m.get(k))
                    .map(val_to_string)
                    .unwrap_or_default();
                ui.label(k);
                ui.label(val);
                ui.end_row();
            }
        });
    });
}

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Essai Control Panel v2",
        NativeOptions::default(),
        Box::new(|_| Ok::<Box<dyn App>, _>(Box::new(MyApp::default()))),
    )
}
