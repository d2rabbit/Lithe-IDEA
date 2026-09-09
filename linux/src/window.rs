//! Lithe native Linux window: GNOME shell over the shared Rust core.
//!
//! The layout uses the canonical libadwaita building blocks —
//! `AdwToolbarView` + `AdwHeaderBar`, `AdwNavigationSplitView` with
//! `AdwNavigationPage`, and `AdwStatusPage` for the empty state — so window
//! metrics, spacing, and title handling follow the platform standard instead
//! of hand-rolled containers. Every product behavior (workspace listing, file
//! read/write, Git state) is a `lithe_core` command; this layer only owns
//! native widgets and dialogs.

use crate::core_bridge;
use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk::Stack;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const OPEN_FOLDER_TITLE: &str = "打开项目文件夹";

struct LitheWindow {
    window: adw::ApplicationWindow,
    stack: Stack,
    files_model: gtk::StringList,
    file_filter: gtk::StringFilter,
    editor: sourceview5::View,
    editor_buffer: sourceview5::Buffer,
    status_label: gtk::Label,
    branch_label: gtk::Label,
    content_page: adw::NavigationPage,
    workspace_root: RefCell<Option<String>>,
    open_file_path: RefCell<Option<String>>,
}

type SharedWindow = Rc<LitheWindow>;

thread_local! {
    static WINDOW: RefCell<Option<SharedWindow>> = const { RefCell::new(None) };
}

/// Opens workspaces passed as positional launch arguments (GApplication open).
pub fn open_files(files: &[gio::File]) {
    WINDOW.with(|slot| {
        let Some(this) = slot.borrow_mut().as_ref().cloned() else {
            return;
        };
        for file in files {
            load_workspace(&this, &path_for(file));
        }
    });
}

fn path_for(file: &gio::File) -> String {
    file.path()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Builds and presents the main window, wiring all behavior to shared-core
/// commands executed off the UI thread.
pub fn create(app: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Lithe")
        .default_width(1150)
        .default_height(760)
        .build();

    let header = adw::HeaderBar::new();
    let open_button = gtk::Button::new();
    open_button.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("folder-open-symbolic")
            .label("打开文件夹")
            .build(),
    ));
    open_button.add_css_class("flat");
    header.pack_start(&open_button);

    let branch_label = gtk::Label::new(None);
    branch_label.set_tooltip_text(Some("当前 Git 分支(lithe-core git.status)"));
    header.pack_end(&branch_label);

    // Welcome state: canonical AdwStatusPage empty state.
    let welcome_open = gtk::Button::new();
    welcome_open.set_child(Some(
        &adw::ButtonContent::builder()
            .icon_name("folder-open-symbolic")
            .label("打开文件夹")
            .build(),
    ));
    welcome_open.add_css_class("suggested-action");
    welcome_open.add_css_class("pill");
    let welcome = adw::StatusPage::builder()
        .icon_name("folder-open-symbolic")
        .title("欢迎使用 Lithe")
        .description("打开一个项目文件夹,开始浏览与编辑。")
        .child(&welcome_open)
        .build();

    // Workbench state: sidebar file list + editor, standard split layout.
    let files_model = gtk::StringList::new(&[]);
    let expression =
        gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string");
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
    let files_scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&files_view)
        .build();
    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("搜索文件…"));
    search_entry.set_margin_top(6);
    search_entry.set_margin_bottom(4);
    search_entry.set_margin_start(6);
    search_entry.set_margin_end(6);
    let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_content.append(&search_entry);
    sidebar_content.append(&files_scroller);
    let sidebar_page = adw::NavigationPage::builder()
        .title("文件")
        .child(&sidebar_content)
        .build();

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

    let status_label = gtk::Label::new(Some("打开一个项目文件夹开始"));
    status_label.set_halign(gtk::Align::Start);
    status_label.set_margin_start(8);
    status_label.set_margin_bottom(2);

    let content_page = adw::NavigationPage::builder().title("Lithe").build();
    let split_view = adw::NavigationSplitView::new();
    split_view.set_sidebar(Some(&sidebar_page));
    split_view.set_content(Some(&content_page));

    content_page.set_child(Some(&editor_scroller));

    let stack = Stack::new();
    stack.add_named(&welcome, Some("welcome"));
    stack.add_named(&split_view, Some("workbench"));
    stack.set_vhomogeneous(false);

    let toolbar_view = adw::ToolbarView::new();
    toolbar_view.add_top_bar(&header);
    toolbar_view.set_content(Some(&stack));
    toolbar_view.add_bottom_bar(&status_label);
    window.set_content(Some(&toolbar_view));

    let this: SharedWindow = Rc::new(LitheWindow {
        window: window.clone(),
        stack,
        files_model,
        file_filter: file_filter.clone(),
        editor: editor.clone(),
        editor_buffer: editor_buffer.clone(),
        status_label: status_label.clone(),
        branch_label: branch_label.clone(),
        content_page: content_page.clone(),
        workspace_root: RefCell::new(None),
        open_file_path: RefCell::new(None),
    });

    {
        let this = this.clone();
        open_button.connect_clicked(move |_| pick_workspace(&this));
    }
    {
        let this = this.clone();
        welcome_open.connect_clicked(move |_| pick_workspace(&this));
    }
    {
        let this = this.clone();
        selection.connect_selection_changed(move |selection, _, _| {
            open_selected(&this, selection);
        });
    }
    {
        // Double-click and Enter open the selected file explicitly.
        let this = this.clone();
        files_view.connect_activate(move |_, position| {
            let Some(path) = this.files_model.string(position).map(|value| value.to_string())
            else {
                return;
            };
            let Some(root) = this.workspace_root.borrow().clone() else {
                return;
            };
            let open_path = path.clone();
            *this.open_file_path.borrow_mut() = Some(path.clone());
            this.status_label.set_text(&format!("读取 {path} …"));
            let editor = this.editor.clone();
            let status_label = this.status_label.clone();
            let editor_buffer = this.editor_buffer.clone();
            glib::spawn_future_local(async move {
                let result = gio::spawn_blocking(move || core_bridge::read_file(&root, &path))
                    .await
                    .unwrap_or_else(|_| Err("后台读取失败".to_string()));
                match result {
                    Ok(text) => {
                        editor_buffer.set_text(&text);
                        apply_highlight(&editor_buffer, &open_path);
                        status_label
                            .set_text(&format!("已打开 {open_path}(Ctrl+S 保存)"));
                    }
                    Err(error) => status_label.set_text(&error),
                }
            });
        });
    }
    {
        let file_filter = file_filter.clone();
        search_entry.connect_search_changed(move |entry| {
            file_filter.set_search(Some(&entry.text()));
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

    window.present();
    WINDOW.with(|slot| *slot.borrow_mut() = Some(this));
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
        load_workspace(&this, &path.to_string_lossy());
    });
    dialog.show();
}

fn load_workspace(this: &SharedWindow, root: &str) {
    *this.workspace_root.borrow_mut() = Some(root.to_string());
    let workspace_name = std::path::Path::new(root)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.to_string());
    this.content_page.set_title(&workspace_name);
    this.window.set_title(Some(&format!("Lithe — {workspace_name}")));
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
}

fn open_selected(this: &SharedWindow, selection: &gtk::SingleSelection) {
    let Some(root) = this.workspace_root.borrow().clone() else {
        return;
    };
    let Some(item) = selection.selected_item().and_downcast::<gtk::StringObject>() else {
        return;
    };
    let path = item.string().to_string();

    *this.open_file_path.borrow_mut() = Some(path.clone());
    this.status_label.set_text(&format!("读取 {path} …"));

    let status_label = this.status_label.clone();
    let editor_buffer = this.editor_buffer.clone();
    let open_path = path.clone();
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

/// Applies syntax highlighting for the file path using the GtkSourceView
/// language registry (guessed from the file name).
fn apply_highlight(buffer: &sourceview5::Buffer, path: &str) {
    let manager = sourceview5::LanguageManager::default();
    if let Some(language) = manager.guess_language(Some(path), None) {
        buffer.set_language(Some(&language));
    }
}
