//! Lithe native Linux window: GNOME shell over the shared Rust core.
//!
//! Every product behavior (workspace listing, file read/write, Git state) is a
//! `lithe_core` command; this layer only owns native widgets and dialogs.

use crate::core_bridge;
use adw::prelude::*;
use gtk::gio;
use gtk::glib;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

const OPEN_FOLDER_TITLE: &str = "打开项目文件夹";

struct LitheWindow {
    window: adw::ApplicationWindow,
    title: gtk::Label,
    files_model: gtk::StringList,
    editor: gtk::TextView,
    status_label: gtk::Label,
    branch_label: gtk::Label,
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
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Lithe")
        .default_width(1150)
        .default_height(760)
        .build();

    let header = adw::HeaderBar::new();
    let open_button = gtk::Button::from_icon_name("folder-open-symbolic");
    open_button.set_tooltip_text(Some(OPEN_FOLDER_TITLE));
    header.pack_start(&open_button);

    let title = gtk::Label::new(Some("Lithe"));
    header.set_title_widget(Some(&title));

    let branch_label = gtk::Label::new(None);
    branch_label.set_tooltip_text(Some("当前 Git 分支(lithe-core git.status)"));
    header.pack_end(&branch_label);

    // Left: workspace file list fed by `workspace.snapshot`.
    let files_model = gtk::StringList::new(&[]);
    let selection = gtk::SingleSelection::new(Some(files_model.clone()));
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
        .min_content_width(280)
        .child(&files_view)
        .build();

    // Right: monospace editor plus a status row.
    let editor = gtk::TextView::new();
    editor.set_monospace(true);
    editor.set_left_margin(8);
    editor.set_right_margin(8);
    editor.set_top_margin(6);
    editor.set_bottom_margin(6);
    let editor_scroller = gtk::ScrolledWindow::builder()
        .child(&editor)
        .hexpand(true)
        .vexpand(true)
        .build();

    let status_label = gtk::Label::new(Some("打开一个项目文件夹开始"));
    status_label.set_halign(gtk::Align::Start);
    status_label.set_margin_start(8);
    status_label.set_margin_bottom(2);

    let editor_pane = gtk::Box::new(gtk::Orientation::Vertical, 4);
    editor_pane.append(&editor_scroller);
    editor_pane.append(&status_label);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&files_scroller));
    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);
    paned.set_end_child(Some(&editor_pane));
    paned.set_resize_end_child(true);
    paned.set_shrink_end_child(false);
    paned.set_vexpand(true);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&paned);
    window.set_content(Some(&content));

    let this: SharedWindow = Rc::new(LitheWindow {
        window: window.clone(),
        title: title.clone(),
        files_model,
        editor: editor.clone(),
        status_label: status_label.clone(),
        branch_label: branch_label.clone(),
        workspace_root: RefCell::new(None),
        open_file_path: RefCell::new(None),
    });

    {
        let this = this.clone();
        open_button.connect_clicked(move |_| pick_workspace(&this));
    }
    {
        let this = this.clone();
        selection.connect_selection_changed(move |selection, _, _| {
            open_selected(&this, selection);
        });
    }
    let controller = gtk::EventControllerKey::new();
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let this = this.clone();
        controller.connect_key_pressed(move |_, key, _, modifier| {
            if modifier.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                && matches!(key, gtk::gdk::Key::s | gtk::gdk::Key::S)
            {
                save_current_file(&this);
                return glib::signal::Propagation::Stop;
            }
            glib::signal::Propagation::Proceed
        });
    }
    this.editor.add_controller(controller);

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
            load_workspace(&this, &path.to_string_lossy());
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
    this.title.set_text(&format!("Lithe — {workspace_name}"));
    this.status_label.set_text(&format!("工作区:{root}"));

    {
        let root = root.to_string();
        let model = this.files_model.clone();
        let status_label = this.status_label.clone();
        glib::spawn_future_local(async move {
            // After the blocking call completes, the continuation runs back on
            // the main context, so touching widgets here is safe.
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
    let Some(item) = selection
        .selected_item()
        .and_downcast::<gtk::StringObject>()
    else {
        return;
    };
    let path = item.string().to_string();

    *this.open_file_path.borrow_mut() = Some(path.clone());
    this.status_label.set_text(&format!("读取 {path} …"));

    let editor = this.editor.clone();
    let status_label = this.status_label.clone();
    let open_path = path.clone();
    glib::spawn_future_local(async move {
        let result = gio::spawn_blocking(move || core_bridge::read_file(&root, &path))
            .await
            .unwrap_or_else(|_| Err("后台读取失败".to_string()));
        match result {
            Ok(text) => {
                editor.buffer().set_text(&text);
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
    let buffer = this.editor.buffer();
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
