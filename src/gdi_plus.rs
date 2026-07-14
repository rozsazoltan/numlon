use std::{ffi::c_void, ptr};

use windows_sys::Win32::{
    Foundation::COLORREF,
    Graphics::Gdi::HDC,
};

const STATUS_OK: i32 = 0;
const FILL_MODE_ALTERNATE: i32 = 0;
const UNIT_PIXEL: i32 = 2;
const SMOOTHING_MODE_ANTI_ALIAS_8X8: i32 = 6;
const PIXEL_OFFSET_MODE_HALF: i32 = 4;
const COMPOSITING_QUALITY_GAMMA_CORRECTED: i32 = 3;

#[repr(C)]
struct GdiplusStartupInput {
    gdiplus_version: u32,
    debug_event_callback: *const c_void,
    suppress_background_thread: i32,
    suppress_external_codecs: i32,
}

pub struct Session {
    token: usize,
}

impl Session {
    pub fn start() -> Option<Self> {
        let input = GdiplusStartupInput {
            gdiplus_version: 1,
            debug_event_callback: ptr::null(),
            suppress_background_thread: 0,
            suppress_external_codecs: 1,
        };
        let mut token = 0usize;
        let status = unsafe { GdiplusStartup(&mut token, &input, ptr::null_mut()) };

        (status == STATUS_OK && token != 0).then_some(Self { token })
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.token);
        }
    }
}

pub unsafe fn draw_rounded_rect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    fill: COLORREF,
    border: COLORREF,
) -> bool {
    let width = right - left;
    let height = bottom - top;
    if hdc.is_null() || width <= 0 || height <= 0 {
        return false;
    }

    let Some(graphics) = Graphics::create(hdc) else {
        return false;
    };
    graphics.configure();

    let x = left as f32 + 0.5;
    let y = top as f32 + 0.5;
    let width = (width - 1).max(1) as f32;
    let height = (height - 1).max(1) as f32;
    let radius = (radius.max(1) as f32).min(width / 2.0).min(height / 2.0);

    let Some(path) = PathHandle::rounded_rect(x, y, width, height, radius) else {
        return false;
    };
    let Some(brush) = Brush::create(colorref_to_argb(fill)) else {
        return false;
    };
    let Some(pen) = Pen::create(colorref_to_argb(border), 1.0) else {
        return false;
    };

    GdipFillPath(graphics.raw, brush.raw.cast(), path.raw) == STATUS_OK
        && GdipDrawPath(graphics.raw, pen.raw, path.raw) == STATUS_OK
}

pub unsafe fn draw_ellipse(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    fill: COLORREF,
    border: COLORREF,
) -> bool {
    let width = right - left;
    let height = bottom - top;
    if hdc.is_null() || width <= 0 || height <= 0 {
        return false;
    }

    let Some(graphics) = Graphics::create(hdc) else {
        return false;
    };
    graphics.configure();

    let Some(brush) = Brush::create(colorref_to_argb(fill)) else {
        return false;
    };
    let Some(pen) = Pen::create(colorref_to_argb(border), 1.0) else {
        return false;
    };

    let x = left as f32 + 0.5;
    let y = top as f32 + 0.5;
    let width = (width - 1).max(1) as f32;
    let height = (height - 1).max(1) as f32;

    GdipFillEllipse(graphics.raw, brush.raw.cast(), x, y, width, height) == STATUS_OK
        && GdipDrawEllipse(graphics.raw, pen.raw, x, y, width, height) == STATUS_OK
}

pub unsafe fn fill_rounded_rect(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    radius: i32,
    fill: COLORREF,
) -> bool {
    let width = right - left;
    let height = bottom - top;
    if hdc.is_null() || width <= 0 || height <= 0 {
        return false;
    }

    let Some(graphics) = Graphics::create(hdc) else {
        return false;
    };
    graphics.configure();

    let width = width as f32;
    let height = height as f32;
    let radius = (radius.max(1) as f32).min(width / 2.0).min(height / 2.0);

    let Some(path) = PathHandle::rounded_rect(
        left as f32,
        top as f32,
        width,
        height,
        radius,
    ) else {
        return false;
    };
    let Some(brush) = Brush::create(colorref_to_argb(fill)) else {
        return false;
    };

    GdipFillPath(graphics.raw, brush.raw.cast(), path.raw) == STATUS_OK
}

pub unsafe fn fill_ellipse(
    hdc: HDC,
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
    fill: COLORREF,
) -> bool {
    let width = right - left;
    let height = bottom - top;
    if hdc.is_null() || width <= 0 || height <= 0 {
        return false;
    }

    let Some(graphics) = Graphics::create(hdc) else {
        return false;
    };
    graphics.configure();

    let Some(brush) = Brush::create(colorref_to_argb(fill)) else {
        return false;
    };

    GdipFillEllipse(
        graphics.raw,
        brush.raw.cast(),
        left as f32,
        top as f32,
        width as f32,
        height as f32,
    ) == STATUS_OK
}

fn colorref_to_argb(color: COLORREF) -> u32 {
    let red = color & 0xff;
    let green = (color >> 8) & 0xff;
    let blue = (color >> 16) & 0xff;

    0xff00_0000 | (red << 16) | (green << 8) | blue
}

struct Graphics {
    raw: *mut c_void,
}

impl Graphics {
    unsafe fn create(hdc: HDC) -> Option<Self> {
        let mut raw = ptr::null_mut();
        (GdipCreateFromHDC(hdc, &mut raw) == STATUS_OK && !raw.is_null()).then_some(Self { raw })
    }

    unsafe fn configure(&self) {
        let _ = GdipSetSmoothingMode(self.raw, SMOOTHING_MODE_ANTI_ALIAS_8X8);
        let _ = GdipSetPixelOffsetMode(self.raw, PIXEL_OFFSET_MODE_HALF);
        let _ = GdipSetCompositingQuality(
            self.raw,
            COMPOSITING_QUALITY_GAMMA_CORRECTED,
        );
    }
}

impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeleteGraphics(self.raw);
        }
    }
}

struct Brush {
    raw: *mut c_void,
}

impl Brush {
    unsafe fn create(color: u32) -> Option<Self> {
        let mut raw = ptr::null_mut();
        (GdipCreateSolidFill(color, &mut raw) == STATUS_OK && !raw.is_null()).then_some(Self { raw })
    }
}

impl Drop for Brush {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeleteBrush(self.raw);
        }
    }
}

struct Pen {
    raw: *mut c_void,
}

impl Pen {
    unsafe fn create(color: u32, width: f32) -> Option<Self> {
        let mut raw = ptr::null_mut();
        (GdipCreatePen1(color, width, UNIT_PIXEL, &mut raw) == STATUS_OK && !raw.is_null())
            .then_some(Self { raw })
    }
}

impl Drop for Pen {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeletePen(self.raw);
        }
    }
}

struct PathHandle {
    raw: *mut c_void,
}

impl PathHandle {
    unsafe fn rounded_rect(
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> Option<Self> {
        let mut raw = ptr::null_mut();
        if GdipCreatePath(FILL_MODE_ALTERNATE, &mut raw) != STATUS_OK || raw.is_null() {
            return None;
        }

        let path = Self { raw };
        let diameter = radius * 2.0;
        let right = x + width;
        let bottom = y + height;

        let statuses = [
            GdipAddPathArc(path.raw, x, y, diameter, diameter, 180.0, 90.0),
            GdipAddPathArc(
                path.raw,
                right - diameter,
                y,
                diameter,
                diameter,
                270.0,
                90.0,
            ),
            GdipAddPathArc(
                path.raw,
                right - diameter,
                bottom - diameter,
                diameter,
                diameter,
                0.0,
                90.0,
            ),
            GdipAddPathArc(
                path.raw,
                x,
                bottom - diameter,
                diameter,
                diameter,
                90.0,
                90.0,
            ),
            GdipClosePathFigure(path.raw),
        ];

        statuses
            .iter()
            .all(|status| *status == STATUS_OK)
            .then_some(path)
    }
}

impl Drop for PathHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeletePath(self.raw);
        }
    }
}

#[link(name = "gdiplus")]
unsafe extern "system" {
    fn GdiplusStartup(
        token: *mut usize,
        input: *const GdiplusStartupInput,
        output: *mut c_void,
    ) -> i32;
    fn GdiplusShutdown(token: usize);

    fn GdipCreateFromHDC(hdc: HDC, graphics: *mut *mut c_void) -> i32;
    fn GdipDeleteGraphics(graphics: *mut c_void) -> i32;
    fn GdipSetSmoothingMode(graphics: *mut c_void, smoothing_mode: i32) -> i32;
    fn GdipSetPixelOffsetMode(graphics: *mut c_void, pixel_offset_mode: i32) -> i32;
    fn GdipSetCompositingQuality(graphics: *mut c_void, compositing_quality: i32) -> i32;

    fn GdipCreateSolidFill(color: u32, brush: *mut *mut c_void) -> i32;
    fn GdipDeleteBrush(brush: *mut c_void) -> i32;

    fn GdipCreatePen1(color: u32, width: f32, unit: i32, pen: *mut *mut c_void) -> i32;
    fn GdipDeletePen(pen: *mut c_void) -> i32;

    fn GdipCreatePath(fill_mode: i32, path: *mut *mut c_void) -> i32;
    fn GdipDeletePath(path: *mut c_void) -> i32;
    fn GdipAddPathArc(
        path: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) -> i32;
    fn GdipClosePathFigure(path: *mut c_void) -> i32;

    fn GdipFillPath(graphics: *mut c_void, brush: *mut c_void, path: *mut c_void) -> i32;
    fn GdipDrawPath(graphics: *mut c_void, pen: *mut c_void, path: *mut c_void) -> i32;
    fn GdipFillEllipse(
        graphics: *mut c_void,
        brush: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> i32;
    fn GdipDrawEllipse(
        graphics: *mut c_void,
        pen: *mut c_void,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> i32;
}
