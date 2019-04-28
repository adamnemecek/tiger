use failure::Error;
use std::fs::File;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::export::*;
use crate::sheet::*;
use crate::state::*;

const SHEET_FILE_EXTENSION: &str = "tiger";
const TEMPLATE_FILE_EXTENSION: &str = "liquid";
const IMAGE_IMPORT_FILE_EXTENSIONS: &str = "png;tga;bmp";
const IMAGE_EXPORT_FILE_EXTENSIONS: &str = "png";

#[derive(Clone, Debug)]
pub struct AppState {
    tabs: Vec<Tab>,
    current_tab: Option<PathBuf>,
    clock: Duration,
}

impl AppState {
    pub fn new() -> AppState {
        AppState {
            tabs: vec![],
            current_tab: None,
            clock: Duration::new(0, 0),
        }
    }

    pub fn tick(&mut self, delta: Duration) {
        self.clock += delta;
        if let Some(tab) = self.get_current_tab_mut() {
            tab.tick(delta);
        }
    }

    pub fn get_clock(&self) -> Duration {
        self.clock
    }

    fn is_opened<T: AsRef<Path>>(&self, path: T) -> bool {
        self.tabs.iter().any(|t| t.source == path.as_ref())
    }

    pub fn get_current_tab(&self) -> Option<&Tab> {
        if let Some(current_path) = &self.current_tab {
            self.tabs.iter().find(|d| &d.source == current_path)
        } else {
            None
        }
    }

    fn get_current_tab_mut(&mut self) -> Option<&mut Tab> {
        if let Some(current_path) = &self.current_tab {
            self.tabs.iter_mut().find(|d| &d.source == current_path)
        } else {
            None
        }
    }

    fn get_tab<T: AsRef<Path>>(&mut self, path: T) -> Option<&Tab> {
        self.tabs.iter().find(|d| d.source == path.as_ref())
    }

    fn get_tab_mut<T: AsRef<Path>>(&mut self, path: T) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|d| d.source == path.as_ref())
    }

    pub fn tabs_iter(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter()
    }

    fn end_new_document<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Error> {
        match self.get_tab_mut(&path) {
            Some(d) => *d = Tab::new(path.as_ref()),
            None => {
                let tab = Tab::new(path.as_ref());
                self.add_tab(tab);
            }
        }
        self.current_tab = Some(path.as_ref().to_owned());
        Ok(())
    }

    fn end_open_document<T: AsRef<Path>>(&mut self, path: T) -> Result<(), Error> {
        if self.get_tab(&path).is_none() {
            let tab = Tab::open(&path)?;
            self.add_tab(tab);
        }
        self.current_tab = Some(path.as_ref().to_path_buf());
        Ok(())
    }

    fn relocate_document<T: AsRef<Path>, U: AsRef<Path>>(
        &mut self,
        from: T,
        to: U,
    ) -> Result<(), Error> {
        for tab in &mut self.tabs {
            if tab.source == from.as_ref() {
                tab.source = to.as_ref().to_path_buf();
                if Some(from.as_ref().to_path_buf()) == self.current_tab {
                    self.current_tab = Some(to.as_ref().to_path_buf());
                }
                return Ok(());
            }
        }
        Err(StateError::DocumentNotFound.into())
    }

    fn add_tab(&mut self, added_tab: Tab) {
        assert!(!self.is_opened(&added_tab.source));
        self.tabs.push(added_tab);
    }

    fn close_current_document(&mut self) -> Result<(), Error> {
        let tab = self.get_current_tab().ok_or(StateError::NoDocumentOpen)?;
        let index = self
            .tabs
            .iter()
            .position(|d| d as *const Tab == tab as *const Tab)
            .ok_or(StateError::DocumentNotFound)?;
        self.tabs.remove(index);
        self.current_tab = if self.tabs.is_empty() {
            None
        } else {
            Some(
                self.tabs[std::cmp::min(index, self.tabs.len() - 1)]
                    .source
                    .clone(),
            )
        };
        Ok(())
    }

    fn close_all_documents(&mut self) {
        self.tabs.clear();
        self.current_tab = None;
    }

    fn save_all_documents(&mut self) -> Result<(), Error> {
        for tab in &mut self.tabs {
            tab.document.save(&tab.source)?;
        }
        Ok(())
    }

    fn process_app_command(&mut self, command: &AppCommand) -> Result<(), Error> {
        use AppCommand::*;

        match command {
            EndNewDocument(p) => self.end_new_document(p)?,
            EndOpenDocument(p) => self.end_open_document(p)?,
            RelocateDocument(from, to) => self.relocate_document(from, to)?,
            FocusDocument(p) => {
                if self.is_opened(&p) {
                    self.current_tab = Some(p.clone());
                }
            }
            CloseCurrentDocument => self.close_current_document()?,
            CloseAllDocuments => self.close_all_documents(),
            SaveAllDocuments => self.save_all_documents()?,
            Undo => self
                .get_current_tab_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .undo()?,
            Redo => self
                .get_current_tab_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .redo()?,
        }

        Ok(())
    }

    fn process_tab_command(&mut self, command: &TabCommand) -> Result<(), Error> {
        use TabCommand::*;

        let mut tab = match command {
            EndImport(p, _)
            | EndSetExportTextureDestination(p, _)
            | EndSetExportMetadataDestination(p, _)
            | EndSetExportMetadataPathsRoot(p, _)
            | EndSetExportFormat(p, _) => self.get_tab(p),
            _ => self.get_current_tab(),
        }
        .cloned();

        match command {
            EndImport(_, f) => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .import(f),
            BeginExportAs => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .document
                .begin_export_as(),
            CancelExportAs => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .cancel_export_as(),
            EndSetExportTextureDestination(_, d) => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .end_set_export_texture_destination(d)?,
            EndSetExportMetadataDestination(_, d) => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .end_set_export_metadata_destination(d)?,
            EndSetExportMetadataPathsRoot(_, d) => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .end_set_export_metadata_paths_root(d)?,
            EndSetExportFormat(_, f) => tab
                .as_mut()
                .ok_or(StateError::DocumentNotFound)?
                .document
                .end_set_export_format(f.clone())?,
            EndExportAs => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .document
                .end_export_as()?,
            SwitchToContentTab(t) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .switch_to_content_tab(*t),
            SelectFrame(p) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_frame(&p)?,
            SelectAnimation(a) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_animation(&a)?,
            SelectHitbox(h) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_hitbox(&h)?,
            SelectAnimationFrame(af) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_animation_frame(*af)?,
            SelectPrevious => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_previous()?,
            SelectNext => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .select_next()?,
            EditFrame(p) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .edit_frame(&p)?,
            EditAnimation(a) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .edit_animation(&a)?,
            CreateAnimation => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .create_animation()?,
            BeginFrameDrag(f) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_frame_drag(f)?,
            EndFrameDrag => {
                tab.as_mut()
                    .ok_or(StateError::NoDocumentOpen)?
                    .transient
                    .content_frame_being_dragged = None
            }
            InsertAnimationFrameBefore(f, n) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .insert_animation_frame_before(f, *n)?,
            ReorderAnimationFrame(a, b) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .reorder_animation_frame(*a, *b)?,
            BeginAnimationFrameDurationDrag(a) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_animation_frame_duration_drag(*a)?,
            UpdateAnimationFrameDurationDrag(d) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .update_animation_frame_duration_drag(*d)?,
            EndAnimationFrameDurationDrag => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .end_animation_frame_duration_drag(),
            BeginAnimationFrameDrag(a) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_animation_frame_drag(*a)?,
            EndAnimationFrameDrag => {
                tab.as_mut()
                    .ok_or(StateError::NoDocumentOpen)?
                    .transient
                    .timeline_frame_being_dragged = None
            }
            BeginAnimationFrameOffsetDrag(a, m) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_animation_frame_offset_drag(*a, *m)?,
            UpdateAnimationFrameOffsetDrag(o, b) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .update_animation_frame_offset_drag(*o, *b)?,
            EndAnimationFrameOffsetDrag => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .end_animation_frame_offset_drag(),
            WorkbenchZoomIn => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .workbench_zoom_in(),
            WorkbenchZoomOut => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .workbench_zoom_out(),
            WorkbenchResetZoom => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .workbench_reset_zoom(),
            Pan(delta) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .pan(*delta),
            CreateHitbox(p) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .create_hitbox(*p)?,
            BeginHitboxScale(h, a, p) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_hitbox_scale(&h, *a, *p)?,
            UpdateHitboxScale(p) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .update_hitbox_scale(*p)?,
            EndHitboxScale => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .end_hitbox_scale(),
            BeginHitboxDrag(a, m) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_hitbox_drag(&a, *m)?,
            UpdateHitboxDrag(o, b) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .update_hitbox_drag(*o, *b)?,
            EndHitboxDrag => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .end_hitbox_drag(),
            TogglePlayback => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .toggle_playback()?,
            SnapToPreviousFrame => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .snap_to_previous_frame()?,
            SnapToNextFrame => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .snap_to_next_frame()?,
            ToggleLooping => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .toggle_looping()?,
            TimelineZoomIn => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .timeline_zoom_in(),
            TimelineZoomOut => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .timeline_zoom_out(),
            TimelineResetZoom => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .view
                .timeline_reset_zoom(),
            BeginScrub => {
                tab.as_mut()
                    .ok_or(StateError::NoDocumentOpen)?
                    .transient
                    .timeline_scrubbing = true
            }
            UpdateScrub(t) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .update_timeline_scrub(*t)?,
            EndScrub => {
                tab.as_mut()
                    .ok_or(StateError::NoDocumentOpen)?
                    .transient
                    .timeline_scrubbing = false
            }
            NudgeSelection(d, l) => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .nudge_selection(*d, *l)?,
            DeleteSelection => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .delete_selection(),
            BeginRenameSelection => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .begin_rename_selection()?,
            UpdateRenameSelection(n) => {
                tab.as_mut()
                    .ok_or(StateError::NoDocumentOpen)?
                    .transient
                    .rename_buffer = Some(n.to_owned())
            }
            EndRenameSelection => tab
                .as_mut()
                .ok_or(StateError::NoDocumentOpen)?
                .end_rename_selection()?,
        };

        if let Some(tab) = tab {
            if let Some(persistent_tab) = self.get_tab_mut(&tab.source) {
                persistent_tab.record_command(command, tab.document, tab.view, tab.transient);
            }
        }

        Ok(())
    }

    pub fn process_sync_command(&mut self, command: &SyncCommand) -> Result<(), Error> {
        match command {
            SyncCommand::App(c) => self.process_app_command(c),
            SyncCommand::Tab(c) => self.process_tab_command(c),
        }
    }
}

fn begin_new_document() -> Result<CommandBuffer, Error> {
    let mut command_buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) =
        nfd::open_save_dialog(Some(SHEET_FILE_EXTENSION), None)?
    {
        let mut path = std::path::PathBuf::from(path_string);
        path.set_extension(SHEET_FILE_EXTENSION);
        command_buffer.end_new_document(path);
    };
    Ok(command_buffer)
}

fn begin_open_document() -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    match nfd::open_file_multiple_dialog(Some(SHEET_FILE_EXTENSION), None)? {
        nfd::Response::Okay(path_string) => {
            let path = std::path::PathBuf::from(path_string);
            buffer.end_open_document(path);
        }
        nfd::Response::OkayMultiple(path_strings) => {
            for path_string in path_strings {
                let path = std::path::PathBuf::from(path_string);
                buffer.end_open_document(path);
            }
        }
        _ => (),
    };
    Ok(buffer)
}

fn save_as<T: AsRef<Path>>(source: T, document: &Document) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) =
        nfd::open_save_dialog(Some(SHEET_FILE_EXTENSION), None)?
    {
        let mut new_path = std::path::PathBuf::from(path_string);
        new_path.set_extension(SHEET_FILE_EXTENSION);
        buffer.relocate_document(source, &new_path);
        buffer.save(&new_path, document);
    };
    Ok(buffer)
}

fn begin_import<T: AsRef<Path>>(into: T) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    match nfd::open_file_multiple_dialog(Some(IMAGE_IMPORT_FILE_EXTENSIONS), None)? {
        nfd::Response::Okay(path_string) => {
            let path = std::path::PathBuf::from(path_string);
            buffer.end_import(into, path);
        }
        nfd::Response::OkayMultiple(path_strings) => {
            for path_string in &path_strings {
                let path = std::path::PathBuf::from(path_string);
                buffer.end_import(&into, path);
            }
        }
        _ => (),
    };
    Ok(buffer)
}

fn begin_set_export_texture_destination<T: AsRef<Path>>(
    document_path: T,
) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) =
        nfd::open_save_dialog(Some(IMAGE_EXPORT_FILE_EXTENSIONS), None)?
    {
        let texture_destination = std::path::PathBuf::from(path_string);
        buffer.end_set_export_texture_destination(document_path, texture_destination);
    };
    Ok(buffer)
}

fn begin_set_export_metadata_destination<T: AsRef<Path>>(
    document_path: T,
) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) = nfd::open_save_dialog(None, None)? {
        let metadata_destination = std::path::PathBuf::from(path_string);
        buffer.end_set_export_metadata_destination(document_path, metadata_destination);
    };
    Ok(buffer)
}

fn begin_set_export_metadata_paths_root<T: AsRef<Path>>(
    document_path: T,
) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) = nfd::open_pick_folder(None)? {
        let metadata_paths_root = std::path::PathBuf::from(path_string);
        buffer.end_set_export_metadata_paths_root(document_path, metadata_paths_root);
    }
    Ok(buffer)
}

fn begin_set_export_format<T: AsRef<Path>>(document_path: T) -> Result<CommandBuffer, Error> {
    let mut buffer = CommandBuffer::new();
    if let nfd::Response::Okay(path_string) =
        nfd::open_file_dialog(Some(TEMPLATE_FILE_EXTENSION), None)?
    {
        let format = ExportFormat::Template(std::path::PathBuf::from(path_string));
        buffer.end_set_export_format(document_path, format);
    };
    Ok(buffer)
}

fn export(document: &Document) -> Result<(), Error> {
    let export_settings = document
        .get_sheet()
        .get_export_settings()
        .as_ref()
        .ok_or(StateError::NoExistingExportSettings)?;

    // TODO texture export performance is awful
    let packed_sheet = pack_sheet(document.get_sheet())?;
    let exported_data = export_sheet(
        document.get_sheet(),
        &export_settings,
        &packed_sheet.get_layout(),
    )?;

    {
        let mut file = File::create(&export_settings.metadata_destination)?;
        file.write_all(&exported_data.into_bytes())?;
    }
    {
        let mut file = File::create(&export_settings.texture_destination)?;
        packed_sheet.get_texture().write_to(&mut file, image::PNG)?;
    }

    Ok(())
}

pub fn process_async_command(command: &AsyncCommand) -> Result<CommandBuffer, Error> {
    let no_commands = CommandBuffer::new();
    match command {
        AsyncCommand::BeginNewDocument => begin_new_document(),
        AsyncCommand::BeginOpenDocument => begin_open_document(),
        AsyncCommand::Save(p, d) => d.save(p).and(Ok(no_commands)),
        AsyncCommand::SaveAs(p, d) => save_as(p, d),
        AsyncCommand::BeginSetExportTextureDestination(p) => {
            begin_set_export_texture_destination(p)
        }
        AsyncCommand::BeginSetExportMetadataDestination(p) => {
            begin_set_export_metadata_destination(p)
        }
        AsyncCommand::BeginSetExportMetadataPathsRoot(p) => begin_set_export_metadata_paths_root(p),
        AsyncCommand::BeginSetExportFormat(p) => begin_set_export_format(p),
        AsyncCommand::BeginImport(p) => begin_import(p),
        AsyncCommand::Export(d) => export(d).and(Ok(no_commands)),
    }
}
