use failure::Error;
use imgui::*;
use imgui::StyleVar::*;

pub fn draw<'a>(ui: &Ui<'a>) -> Result<(), Error> {
    let (w, h) = ui.frame_size().logical_size;

    ui.main_menu_bar(|| {
        ui.menu(im_str!("File")).build(|| {
            ui.menu_item(im_str!("New Sheet…")).build();
            ui.menu_item(im_str!("Open Sheet…")).build();
            ui.separator();
            ui.menu_item(im_str!("Save")).build();
            ui.menu_item(im_str!("Save As…")).build();
            ui.menu_item(im_str!("Save All")).build();
            ui.separator();
            ui.menu_item(im_str!("Close")).build();
            ui.menu_item(im_str!("Close All")).build();
        });
        ui.menu(im_str!("View")).build(|| {
            ui.menu_item(im_str!("Grid")).build();
            ui.menu_item(im_str!("Hitboxes")).build();
        });
        ui.menu(im_str!("Help")).build(|| {
            ui.menu_item(im_str!("About")).build();
        });
    });

    ui.with_style_vars(&vec![WindowRounding(0.0), WindowBorderSize(0.0)], || {
        ui.window(im_str!("Frames"))
            .size((w as f32 * 0.20, h as f32 - 60.0), ImGuiCond::Always)
            .position((20.0, 30.0), ImGuiCond::FirstUseEver)
            .collapsible(false)
            .resizable(false)
            .movable(false)
            .build(|| {
                ui.text(im_str!("Hello world!"));
            });
    });

    Ok(())
}
