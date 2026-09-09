//! Lithe native Linux window: a GTK4 shell that reproduces the Tauri
//! product's welcome screen and workbench one-to-one — same structure, same
//! texts, same Lithe Dark palette — with every product behavior (workspace
//! listing, file read/write, Git state, recent projects) backed by
//! `lithe_core` commands or local persistence.

use crate::core_bridge;
use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk::Stack;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

const OPEN_FOLDER_TITLE: &str = "打开项目文件夹";

/// Exact values from the Lithe Dark theme
/// (src/extensions/themes/builtin/lithe.json).
const DARK_CSS: &str = r#"
.lithe-root { background-color: #1e1f22; color: #dfe1e5; font-size: 13px; }
.lithe-chrome { background-color: #2b2d30; border-bottom: 1px solid #43454a; }
.lithe-chrome button {
  background-color: transparent; border: none; color: #dfe1e5;
  padding: 4px 6px; min-height: 24px; min-width: 24px; border-radius: 4px;
}
.lithe-chrome button:hover { background-color: #393b40; }
.lithe-window-close:hover { background-color: #e81123; color: #ffffff; }
.lithe-chrome-title { font-weight: 600; color: #dfe1e5; }
.lithe-branch { color: #8b929e; font-size: 12px; }
.lithe-aside {
  background-color: #2b2d30; border-right: 1px solid #43454a;
  padding: 0 12px 12px 12px;
}
.lithe-logo {
  background-color: #3574f0; color: #ffffff; font-weight: 700;
  border-radius: 12px;
}
.lithe-app-name { font-size: 18px; font-weight: 600; color: #dfe1e5; }
.lithe-caption { font-size: 12px; color: #8b929e; }
.lithe-caption-button {
  font-size: 12px; color: #8b929e; padding: 0; min-height: 16px;
  min-width: 0; background-color: transparent; border: none;
}
.lithe-caption-button:hover { color: #dfe1e5; background-color: transparent; }
.lithe-projects-pill {
  background-color: rgba(53, 116, 240, 0.55); border-radius: 6px;
  padding: 8px 14px; color: #ffffff; font-weight: 500;
}
.lithe-projects-pill:hover { background-color: rgba(53, 116, 240, 0.7); }
.lithe-welcome-title { font-size: 20px; font-weight: 600; color: #dfe1e5; }
.lithe-search-row { border-bottom: 1px solid #43454a; }
.lithe-search {
  background-color: #1e1f22; border: 1px solid #43454a; border-radius: 6px;
  padding: 0 10px; color: #dfe1e5;
}
.lithe-search:focus-within { border-color: #3574f0; }
.lithe-search text { color: #dfe1e5; caret-color: #dfe1e5; background: none; }
.lithe-search image { color: #8b929e; }
.lithe-btn-accent { background-color: #3574f0; color: #ffffff; border-radius: 6px; }
.lithe-btn-accent:hover { background-color: #4a83f2; }
.lithe-btn-default { background-color: #393b40; color: #dfe1e5; border-radius: 6px; }
.lithe-btn-default:hover { background-color: #43454a; }
.lithe-recent-row { border-radius: 6px; }
.lithe-recent-row:hover { background-color: rgba(57, 59, 64, 0.65); }
.lithe-recent-name { font-size: 13px; font-weight: 500; color: #dfe1e5; }
.lithe-recent-path { font-size: 13px; color: #8b929e; }
.lithe-avatar {
  color: #ffffff; font-size: 12px; font-weight: 600; border-radius: 8px;
}
.lithe-avatar-0 { background-color: rgba(16, 185, 129, 0.8); }
.lithe-avatar-1 { background-color: rgba(59, 130, 246, 0.85); }
.lithe-avatar-2 { background-color: rgba(249, 115, 22, 0.85); }
.lithe-avatar-3 { background-color: rgba(6, 182, 212, 0.8); }
.lithe-avatar-4 { background-color: rgba(139, 92, 246, 0.8); }
.lithe-empty-title { font-size: 13px; font-weight: 500; color: #dfe1e5; }
.lithe-empty-hint { font-size: 13px; color: #8b929e; }
.lithe-status { padding: 0 8px 2px 8px; font-size: 12px; color: #8b929e; }
.lithe-editor { background-color: #1e1f22; color: #dfe1e5; }
.lithe-filelist { background-color: #1e1f22; color: #dfe1e5; }
"#;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct RecentFolder {
    path: String,
    name: String,
    #[serde(default)]
    last_opened_at: u64,
}

struct LitheWindow {
    window: adw::ApplicationWindow,
    stack: Stack,
    chrome_title: gtk::Label,
    branch_label: gtk::Label,
    files_model: gtk::StringList,
    file_filter: gtk::StringFilter,
    editor_buffer: sourceview5::Buffer,
    status_label: gtk::Label,
    recents_box: gtk::Box,
    recents: RefCell<Vec<RecentFolder>>,
    query: RefCell<String>,
    workspace_root: RefCell<Option<String>>,
    open_file_path: RefCell<Option<String>>,
}

type SharedWindow = Rc<LitheWindow>;

thread_local! {
    static WINDOW: RefCell<Option<SharedWindow>> = const { RefCell::new(None) };
}

/// Creates the main window during `startup`; `open_files` runs afterwards when
/// the application is launched with a workspace path argument.
pub fn create(app: &adw::Application) {
    load_css();
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Lithe")
        .default_width(1150)
        .default_height(760)
        .build();
    window.set_decorated(false);

    // ---- Custom chrome: drag handle + window controls (Tauri-style). ----
    let chrome = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    chrome.add_css_class("lithe-chrome");
    chrome.set_height_request(40);

    let menu_button = gtk::Button::from_icon_name("open-menu-symbolic");
    chrome.append(&menu_button);

    let chrome_title = gtk::Label::new(Some("Lithe"));
    chrome_title.add_css_class("lithe-chrome-title");
    chrome.append(&chrome_title);

    let branch_label = gtk::Label::new(None);
    branch_label.add_css_class("lithe-branch");
    chrome.append(&branch_label);

    let open_folder_header_button = gtk::Button::from_icon_name("folder-open-symbolic");
    open_folder_header_button.set_tooltip_text(Some(OPEN_FOLDER_TITLE));
    chrome.append(&open_folder_header_button);

    let chrome_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chrome_spacer.set_hexpand(true);
    chrome.append(&chrome_spacer);

    let minimize = gtk::Button::from_icon_name("window-minimize-symbolic");
    let maximize = gtk::Button::from_icon_name("window-maximize-symbolic");
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("lithe-window-close");
    chrome.append(&minimize);
    chrome.append(&maximize);
    chrome.append(&close);

    let drag_handle = gtk::WindowHandle::new();
    drag_handle.set_child(Some(&chrome));

    // ---- Welcome screen (1:1 with the Tauri welcome-screen.tsx). ----
    let aside = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aside.add_css_class("lithe-aside");
    aside.set_width_request(240);

    let logo_row = gtk::Box::new(gtk::Orientation::Horizontal, 11);
    logo_row.set_margin_top(28);
    logo_row.set_margin_bottom(30);
    logo_row.set_margin_start(8);
    let logo = gtk::Label::new(Some("LI"));
    logo.add_css_class("lithe-logo");
    logo.set_size_request(42, 42);
    logo.set_halign(gtk::Align::Center);
    logo.set_valign(gtk::Align::Center);
    logo_row.append(&logo);
    let brand_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let brand_name = gtk::Label::new(Some("Lithe"));
    brand_name.set_halign(gtk::Align::Start);
    brand_name.add_css_class("lithe-app-name");
    brand_box.append(&brand_name);
    let version_caption = gtk::Label::new(Some("0.1.0 · Linux"));
    version_caption.set_halign(gtk::Align::Start);
    version_caption.add_css_class("lithe-caption");
    brand_box.append(&version_caption);
    let check_updates = gtk::Button::new();
    check_updates.set_child(Some(&gtk::Label::new(Some("检查更新"))));
    check_updates.add_css_class("lithe-caption-button");
    brand_box.append(&check_updates);
    logo_row.append(&brand_box);
    aside.append(&logo_row);

    let projects_pill = gtk::Button::new();
    projects_pill.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("folder-symbolic")
            .label("项目")
            .build(),
    ));
    projects_pill.add_css_class("lithe-projects-pill");
    aside.append(&projects_pill);

    let aside_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    aside_spacer.set_vexpand(true);
    aside.append(&aside_spacer);

    let settings_button = gtk::Button::new();
    settings_button.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("emblem-system-symbolic")
            .label("设置")
            .build(),
    ));
    settings_button.set_halign(gtk::Align::Start);
    settings_button.add_css_class("lithe-caption-button");
    aside.append(&settings_button);

    let welcome_main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    welcome_main.set_margin_top(40);
    welcome_main.set_margin_start(20);
    welcome_main.set_margin_end(20);
    let welcome_title = gtk::Label::new(Some("欢迎使用 Lithe"));
    welcome_title.add_css_class("lithe-welcome-title");
    welcome_title.set_halign(gtk::Align::Center);
    welcome_main.append(&welcome_title);

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    search_row.add_css_class("lithe-search-row");
    search_row.set_margin_top(32);
    search_row.set_height_request(64);
    search_row.set_valign(gtk::Align::Center);
    let recents_search = gtk::SearchEntry::new();
    recents_search.set_placeholder_text(Some("搜索项目"));
    recents_search.add_css_class("lithe-search");
    recents_search.set_hexpand(true);
    search_row.append(&recents_search);
    let clone_button = gtk::Button::new();
    clone_button.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("git-branch-symbolic")
            .label("克隆")
            .build(),
    ));
    clone_button.add_css_class("lithe-btn-default");
    search_row.append(&clone_button);
    let welcome_open_button = gtk::Button::new();
    welcome_open_button.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("folder-open-symbolic")
            .label("打开")
            .build(),
    ));
    welcome_open_button.add_css_class("lithe-btn-accent");
    search_row.append(&welcome_open_button);
    welcome_main.append(&search_row);

    let recents_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let recents_scroller = gtk::ScrolledWindow::builder()
        .child(&recents_box)
        .hexpand(true)
        .vexpand(true)
        .build();
    welcome_main.append(&recents_scroller);

    let welcome = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    welcome.append(&aside);
    welcome.append(&welcome_main);

    // ---- Workbench: searchable file list + sourceview editor + status. ----
    let files_model = gtk::StringList::new(&[]);
    let expression = gtk::PropertyExpression::new(
        gtk::StringObject::static_type(),
        None::<gtk::Expression>,
        "string",
    );
    let file_filter = gtk::StringFilter::builder()
        .expression(&expression)
        .ignore_case(true)
        .build();
    let filter_model =
        gtk::FilterListModel::new(Some(files_model.clone()), Some(file_filter.clone()));
    let selection = gtk::SingleSelection::new(Some(filter_model));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let label = gtk::Label::new(None);
        label.set_halign(gtk::Align::Start);
        label.set_margin_top(2);
        label.set_margin_bottom(2);
        label.set_margin_start(6);
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(string_object) = item.item().and_downcast::<gtk::StringObject>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk::Label>() else {
            return;
        };
        label.set_text(string_object.string().as_str());
    });
    let files_view = gtk::ListView::new(Some(selection.clone()), Some(factory));
    let files_search = gtk::SearchEntry::new();
    files_search.set_placeholder_text(Some("搜索文件…"));
    files_search.set_margin_top(6);
    files_search.set_margin_bottom(4);
    files_search.set_margin_start(6);
    files_search.set_margin_end(6);
    let files_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    files_list.add_css_class("lithe-filelist");
    files_list.append(&files_search);
    let files_scroller = gtk::ScrolledWindow::builder()
        .child(&files_view)
        .build();
    files_list.append(&files_scroller);

    let editor_buffer = sourceview5::Buffer::new(None);
    let editor = sourceview5::View::with_buffer(&editor_buffer);
    editor.set_monospace(true);
    editor.set_show_line_numbers(true);
    editor.set_highlight_current_line(true);
    editor.set_left_margin(8);
    editor.set_right_margin(8);
    editor.set_top_margin(6);
    editor.set_bottom_margin(6);
    let style_name = if adw::StyleManager::default().is_dark() {
        "Adwaita-dark"
    } else {
        "Adwaita"
    };
    if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(style_name) {
        editor_buffer.set_style_scheme(Some(&scheme));
    }
    let editor_scroller = gtk::ScrolledWindow::builder()
        .child(&editor)
        .hexpand(true)
        .vexpand(true)
        .build();
    editor_scroller.add_css_class("lithe-editor");

    let status_label = gtk::Label::new(Some("打开一个项目文件夹开始"));
    status_label.add_css_class("lithe-status");
    status_label.set_halign(gtk::Align::Start);

    let workbench_split = gtk::Paned::new(gtk::Orientation::Horizontal);
    workbench_split.set_start_child(Some(&files_list));
    workbench_split.set_resize_start_child(false);
    workbench_split.set_shrink_start_child(false);
    workbench_split.set_end_child(Some(&editor_scroller));
    workbench_split.set_resize_end_child(true);
    workbench_split.set_shrink_end_child(false);
    workbench_split.set_position(260);
    workbench_split.set_vexpand(true);

    let workbench = gtk::Box::new(gtk::Orientation::Vertical, 0);
    workbench.append(&workbench_split);
    workbench.append(&status_label);

    let stack = Stack::new();
    stack.add_named(&welcome, Some("welcome"));
    stack.add_named(&workbench, Some("workbench"));
    stack.set_vhomogeneous(false);
    stack.set_hhomogeneous(false);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("lithe-root");
    root.append(&drag_handle);
    root.append(&stack);
    window.set_content(Some(&root));

    let this: SharedWindow = Rc::new(LitheWindow {
        window: window.clone(),
        stack,
        chrome_title: chrome_title.clone(),
        branch_label: branch_label.clone(),
        files_model,
        file_filter: file_filter.clone(),
        editor_buffer: editor_buffer.clone(),
        status_label: status_label.clone(),
        recents_box: recents_box.clone(),
        recents: RefCell::new(load_recents()),
        query: RefCell::new(String::new()),
        workspace_root: RefCell::new(None),
        open_file_path: RefCell::new(None),
    });

    {
        let this = this.clone();
        open_folder_header_button.connect_clicked(move |_| pick_workspace(&this));
    }
    {
        let this = this.clone();
        welcome_open_button.connect_clicked(move |_| pick_workspace(&this));
    }
    {
        let this = this.clone();
        clone_button.connect_clicked(move |_| {
            this.status_label.set_text("克隆仓库将在后续版本提供");
        });
    }
    {
        let this = this.clone();
        settings_button.connect_clicked(move |_| {
            this.status_label.set_text("设置将在后续版本提供");
        });
    }
    {
        check_updates.connect_clicked(move |_| {
            // v1 has no update feed; the interaction stays intentionally inert.
        });
    }
    {
        let window = window.clone();
        minimize.connect_clicked(move |_| window.minimize());
    }
    {
        let window = window.clone();
        maximize.connect_clicked(move |_| {
            if window.is_maximized() {
                window.unmaximize();
            } else {
                window.maximize();
            }
        });
    }
    {
        let window = window.clone();
        close.connect_clicked(move |_| window.destroy());
    }
    {
        let this = this.clone();
        recents_search.connect_search_changed(move |entry| {
            *this.query.borrow_mut() = entry.text().to_string();
            rebuild_recents(&this);
        });
    }
    {
        let this = this.clone();
        selection.connect_selection_changed(move |selection, _, _| {
            open_selected(&this, selection);
        });
    }
    {
        let this = this.clone();
        files_view.connect_activate(move |_, position| {
            let Some(path) = this.files_model.string(position).map(|v| v.to_string()) else {
                return;
            };
            open_in_editor(&this, &path);
        });
    }
    {
        let this = this.clone();
        files_search.connect_search_changed(move |entry| {
            this.file_filter.set_search(Some(&entry.text()));
        });
    }
    {
        let this = this.clone();
        let controller = gtk::EventControllerKey::new();
        controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        controller.connect_key_pressed(move |_, key, _, modifier| {
            if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && matches!(key, gtk::gdk::Key::s | gtk::gdk::Key::S)
            {
                save_current_file(&this);
                return glib::signal::Propagation::Stop;
            }
            glib::signal::Propagation::Proceed
        });
        editor.add_controller(controller);
    }

    rebuild_recents(&this);
    window.present();
    WINDOW.with(|slot| *slot.borrow_mut() = Some(this));
}

/// Opens workspaces passed as positional launch arguments (GApplication open).
pub fn open_files(files: &[gio::File]) {
    WINDOW.with(|slot| {
        let Some(this) = slot.borrow_mut().as_ref().cloned() else {
            return;
        };
        for file in files {
            let Some(path) = file.path() else {
                continue;
            };
            load_workspace(&this, &path.to_string_lossy().as_ref());
        }
    });
}

fn pick_workspace(this: &SharedWindow) {
    let dialog = gtk::FileChooserNative::builder()
        .title(OPEN_FOLDER_TITLE)
        .action(gtk::FileChooserAction::SelectFolder)
        .transient_for(&this.window)
        .modal(true)
        .build();
    let this = this.clone();
    dialog.connect_response(move |dialog, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(file) = dialog.file() else {
            return;
        };
        let Some(path) = file.path() else {
            return;
        };
        load_workspace(&this, &path.to_string_lossy().as_ref());
    });
    dialog.show();
}

fn load_workspace(this: &SharedWindow, root: &str) {
    *this.workspace_root.borrow_mut() = Some(root.to_string());
    remember_folder(this, root);
    let workspace_name = std::path::Path::new(root)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string());
    this.chrome_title
        .set_text(&format!("Lithe — {workspace_name}"));
    this.window
        .set_title(Some(&format!("Lithe — {workspace_name}")));
    this.stack.set_visible_child_name("workbench");
    this.status_label.set_text(&format!("工作区:{root}"));

    {
        let root = root.to_string();
        let model = this.files_model.clone();
        let status_label = this.status_label.clone();
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || core_bridge::workspace_files(&root))
                .await
                .unwrap_or_else(|_| Err("后台工作区扫描失败".to_string()));
            model.splice(0, model.n_items(), &[]);
            match result {
                Ok(files) => {
                    for file in files {
                        model.append(&file);
                    }
                }
                Err(error) => status_label.set_text(&error),
            }
        });
    }

    {
        let root = root.to_string();
        let branch_label = this.branch_label.clone();
        glib::spawn_future_local(async move {
            let branch = gio::spawn_blocking(move || core_bridge::git_branch(&root))
                .await
                .unwrap_or_default();
            branch_label.set_text(branch.as_deref().unwrap_or("no git"));
        });
    }

    rebuild_recents(this);
}

fn open_selected(this: &SharedWindow, selection: &gtk::SingleSelection) {
    let Some(item) = selection.selected_item().and_downcast::<gtk::StringObject>() else {
        return;
    };
    let path = item.string().to_string();
    open_in_editor(this, &path);
}

fn open_in_editor(this: &SharedWindow, path: &str) {
    let Some(root) = this.workspace_root.borrow().clone() else {
        return;
    };
    let path = path.to_string();
    *this.open_file_path.borrow_mut() = Some(path.clone());
    this.status_label.set_text(&format!("读取 {path} …"));

    let status_label = this.status_label.clone();
    let editor_buffer = this.editor_buffer.clone();
    let open_path = path.to_string();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || core_bridge::read_file(&root, &path))
            .await
            .unwrap_or_else(|_| Err("后台读取失败".to_string()));
        match result {
            Ok(text) => {
                editor_buffer.set_text(&text);
                apply_highlight(&editor_buffer, &open_path);
                status_label.set_text(&format!("已打开 {open_path}(Ctrl+S 保存)"));
            }
            Err(error) => status_label.set_text(&error),
        }
    });
}

fn save_current_file(this: &SharedWindow) {
    let Some(root) = this.workspace_root.borrow().clone() else {
        return;
    };
    let Some(path) = this.open_file_path.borrow().clone() else {
        this.status_label.set_text("没有已打开的文件");
        return;
    };
    let buffer = &this.editor_buffer;
    let (start, end) = buffer.bounds();
    let text = buffer.text(&start, &end, false);

    this.status_label.set_text(&format!("保存 {path} …"));
    let status_label = this.status_label.clone();
    let saved_path = path.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || core_bridge::write_file(&root, &path, &text))
            .await
            .unwrap_or_else(|_| Err("后台保存失败".to_string()));
        let message = match result {
            Ok(()) => format!("已保存 {saved_path}"),
            Err(error) => error,
        };
        status_label.set_text(&message);
    });
}

/// Rebuilds the recent-project rows from the persisted list filtered by the
/// current search query, mirroring welcome-screen.tsx.
fn rebuild_recents(this: &SharedWindow) {
    let query = this.query.borrow().trim().to_lowercase();
    let mut recents = this.recents.borrow().clone();
    recents.sort_by(|left, right| right.last_opened_at.cmp(&left.last_opened_at));
    if !query.is_empty() {
        recents.retain(|entry| {
            entry.name.to_lowercase().contains(&query)
                || entry.path.to_lowercase().contains(&query)
        });
    }

    while let Some(child) = this.recents_box.first_child() {
        this.recents_box.remove(&child);
    }

    if recents.is_empty() {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty.set_valign(gtk::Align::Center);
        empty.set_margin_top(96);
        let icon = gtk::Image::from_icon_name("folder-open-symbolic");
        icon.set_pixel_size(28);
        icon.add_css_class("lithe-caption");
        let title = gtk::Label::new(Some("暂无最近项目"));
        title.add_css_class("lithe-empty-title");
        let hint = gtk::Label::new(Some("打开文件夹以开始使用。"));
        hint.add_css_class("lithe-empty-hint");
        empty.append(&icon);
        empty.append(&title);
        empty.append(&hint);
        empty.set_halign(gtk::Align::Center);
        this.recents_box.append(&empty);
        return;
    }

    for entry in recents {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("lithe-recent-row");
        row.set_height_request(52);

        let avatar = gtk::Label::new(Some(&project_initials(&entry.name)));
        avatar.add_css_class("lithe-avatar");
        avatar.add_css_class(avatar_css_class(&entry.path));
        avatar.set_size_request(34, 34);
        avatar.set_halign(gtk::Align::Center);
        avatar.set_valign(gtk::Align::Center);
        row.append(&avatar);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let name_label = gtk::Label::new(Some(&entry.name));
        name_label.add_css_class("lithe-recent-name");
        name_label.set_halign(gtk::Align::Start);
        text_box.append(&name_label);
        let path_label = gtk::Label::new(Some(&entry.path));
        path_label.add_css_class("lithe-recent-path");
        path_label.set_halign(gtk::Align::Start);
        text_box.append(&path_label);
        row.append(&text_box);

        let click = gtk::GestureClick::new();
        {
            let this = this.clone();
            let root = entry.path.clone();
            click.connect_released(move |_, _, _, _| {
                load_workspace(&this, &root);
            });
        }
        row.add_controller(click);
        this.recents_box.append(&row);
    }
}

fn remember_folder(this: &SharedWindow, path: &str) {
    let mut recents = this.recents.borrow_mut();
    recents.retain(|entry| entry.path != path);
    recents.insert(
        0,
        RecentFolder {
            path: path.to_string(),
            name: std::path::Path::new(path)
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string()),
            last_opened_at: now_millis(),
        },
    );
    recents.truncate(20);
    recents.sort_by(|left, right| right.last_opened_at.cmp(&left.last_opened_at));
    save_recents(&recents);
}

fn apply_highlight(buffer: &sourceview5::Buffer, path: &str) {
    let manager = sourceview5::LanguageManager::default();
    if let Some(language) = manager.guess_language(Some(path), None) {
        buffer.set_language(Some(&language));
    }
}


fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(DARK_CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn project_initials(name: &str) -> String {
    let words: Vec<&str> = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .collect();
    let initials: String = words
        .iter()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect();
    if initials.is_empty() {
        "LI".to_string()
    } else {
        initials.to_uppercase()
    }
}

/// Project avatar colors copied from welcome-screen.tsx projectColors.
fn avatar_css_class(path: &str) -> &'static str {
    let hash: usize = path.chars().map(|character| character as usize).sum();
    [
        "lithe-avatar-0",
        "lithe-avatar-1",
        "lithe-avatar-2",
        "lithe-avatar-3",
        "lithe-avatar-4",
    ][hash % 5]
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn recents_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(
        std::path::PathBuf::from(home)
            .join(".config")
            .join("lithe-linux")
            .join("recents.json"),
    )
}

fn load_recents() -> Vec<RecentFolder> {
    let Some(path) = recents_path() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

fn save_recents(recents: &[RecentFolder]) {
    let Some(path) = recents_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(recents) {
        let _ = std::fs::write(path, text);
    }
}
