#![cfg(windows)]

use tauri::image::Image;
use windows::{
    core::{w, Owned},
    Win32::{
        Foundation::{COLORREF, SIZE},
        Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, GdiFlush,
            GetTextExtentPoint32W, SelectObject, SetBkMode, SetTextColor, TextOutW, ANSI_CHARSET,
            ANTIALIASED_QUALITY, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLIP_DEFAULT_PRECIS,
            CLR_INVALID, DIB_RGB_COLORS, FW_SEMIBOLD, HBITMAP, HDC, HFONT, HGDIOBJ, OUT_TT_PRECIS,
            TRANSPARENT,
        },
    },
};

const TILE: [u8; 4] = [0x25, 0x32, 0x3e, 255];
const TEXT: COLORREF = COLORREF(0x00ef_f4f5);

/// Integer usage; the tooltip carries the percent unit so tray-size digits stay legible.
pub fn icon(percent: Option<u8>) -> Result<Image<'static>, String> {
    render(percent, 32)
}

struct Surface {
    dc: HDC,
    bitmap: Owned<HBITMAP>,
    font: Owned<HFONT>,
    previous_bitmap: HGDIOBJ,
    previous_font: HGDIOBJ,
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            if !self.previous_font.is_invalid() {
                SelectObject(self.dc, self.previous_font);
            }
            if !self.previous_bitmap.is_invalid() {
                SelectObject(self.dc, self.previous_bitmap);
            }
            if !self.dc.is_invalid() {
                let _ = DeleteDC(self.dc);
            }
            // Owned fonts/bitmaps drop after the DC, even if restoring a selection failed.
        }
    }
}

impl Surface {
    fn new() -> Result<Self, String> {
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err("CreateCompatibleDC failed for the tray icon".into());
        }
        Ok(Self {
            dc,
            bitmap: Owned::default(),
            font: Owned::default(),
            previous_bitmap: HGDIOBJ::default(),
            previous_font: HGDIOBJ::default(),
        })
    }

    fn draw_text(&mut self, text: &[u16], side: i32) -> Result<SIZE, String> {
        unsafe {
            for height in (1..=side - 2).rev() {
                let font = Owned::new(CreateFontW(
                    -height,
                    0,
                    0,
                    0,
                    FW_SEMIBOLD.0 as i32,
                    0,
                    0,
                    0,
                    ANSI_CHARSET,
                    OUT_TT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    ANTIALIASED_QUALITY,
                    0,
                    w!("Segoe UI Semibold"),
                ));
                if font.is_invalid() {
                    return Err("CreateFontW failed for the tray icon".into());
                }
                let previous = SelectObject(self.dc, HGDIOBJ(font.0));
                if previous.is_invalid() {
                    return Err("Could not select the tray font".into());
                }
                if self.font.is_invalid() {
                    self.previous_font = previous;
                }
                self.font = font;
                let mut extent = SIZE::default();
                GetTextExtentPoint32W(self.dc, text, &mut extent)
                    .ok()
                    .map_err(|e| format!("Could not measure tray text: {e}"))?;
                if extent.cx <= side - 2 && extent.cy <= side - 2 {
                    TextOutW(
                        self.dc,
                        (side - extent.cx) / 2,
                        (side - extent.cy) / 2,
                        text,
                    )
                    .ok()
                    .map_err(|e| format!("Could not draw tray text: {e}"))?;
                    return Ok(extent);
                }
            }
        }
        Err("Tray text does not fit".into())
    }
}

fn render(percent: Option<u8>, size: u32) -> Result<Image<'static>, String> {
    if percent.is_some_and(|p| p > 100) || !(16..=32).contains(&size) {
        return Err("Tray icons require a percentage in 0..=100 and a size in 16..=32".into());
    }
    let text: Vec<u16> = percent
        .map(|p| p.to_string())
        .unwrap_or_else(|| "—".into())
        .encode_utf16()
        .collect();
    let side = size as i32;
    let length = size as usize * size as usize * 4;
    let mut rgba = vec![0; length];
    unsafe {
        let mut surface = Surface::new()?;
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: side,
                biHeight: -side,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        surface.bitmap = Owned::new(
            CreateDIBSection(Some(surface.dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
                .map_err(|e| format!("Could not allocate the tray bitmap: {e}"))?,
        );
        if bits.is_null() {
            return Err("Tray bitmap has no pixel storage".into());
        }
        surface.previous_bitmap = SelectObject(surface.dc, HGDIOBJ(surface.bitmap.0));
        if surface.previous_bitmap.is_invalid() {
            return Err("Could not select the tray bitmap".into());
        }
        for pixel in std::slice::from_raw_parts_mut(bits.cast::<u8>(), length)
            .as_chunks_mut::<4>()
            .0
        {
            pixel.copy_from_slice(&[TILE[2], TILE[1], TILE[0], 0]);
        }
        if SetBkMode(surface.dc, TRANSPARENT) == 0
            || SetTextColor(surface.dc, TEXT).0 == CLR_INVALID
        {
            return Err("Could not configure tray text colors".into());
        }
        surface.draw_text(&text, side)?;
        // GDI batches writes; finish them before accessing DIB memory through its pointer.
        GdiFlush()
            .ok()
            .map_err(|e| format!("Could not finish drawing the tray icon: {e}"))?;
        let bgra = std::slice::from_raw_parts(bits.cast::<u8>(), length);
        let radius = side / 6;
        for (index, pixel) in rgba.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let x = index as i32 % side;
            let y = index as i32 / side;
            let dx = x - x.clamp(radius, side - 1 - radius);
            let dy = y - y.clamp(radius, side - 1 - radius);
            if dx * dx + dy * dy <= radius * radius {
                let offset = index * 4;
                pixel.copy_from_slice(&[bgra[offset + 2], bgra[offset + 1], bgra[offset], 255]);
            }
        }
    }
    Ok(Image::new_owned(rgba, size, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetGuiResources, GR_GDIOBJECTS};

    static GDI_TEST: Mutex<()> = Mutex::new(());

    #[test]
    fn renders_every_percentage_and_unknown_without_leaking_gdi_objects() {
        let _guard = GDI_TEST.lock().unwrap();
        icon(Some(67)).unwrap();
        let before = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        for size in [16, 20, 24, 32] {
            for percent in (0..=100).map(Some).chain([None]) {
                let image = render(percent, size).unwrap();
                assert_eq!((image.width(), image.height()), (size, size));
                let pixels = image.rgba();
                assert_eq!(pixels.len(), size as usize * size as usize * 4);
                for corner in [0, size - 1, size * (size - 1), size * size - 1] {
                    assert_eq!(&pixels[corner as usize * 4..][..4], &[0, 0, 0, 0]);
                }
                assert!(pixels.as_chunks::<4>().0.contains(&TILE));
                assert!(pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] > 180 && p[3] == 255));
                assert!(pixels
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[3] == 0 || p[3] == 255));
            }
        }
        let after = unsafe { GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS) };
        if before > 0 && after > 0 {
            assert!(
                after <= before + 2,
                "GDI handles grew from {before} to {after}"
            );
        }
        assert_eq!(icon(None).unwrap().width(), 32);
    }

    #[test]
    fn measures_complete_labels_inside_16px_with_the_real_gdi_font() {
        let _guard = GDI_TEST.lock().unwrap();
        let mut surface = Surface::new().unwrap();
        for percent in 0..=100 {
            let text: Vec<_> = percent.to_string().encode_utf16().collect();
            let extent = surface.draw_text(&text, 16).unwrap();
            assert!(extent.cx > 0 && extent.cx <= 14, "{percent}: {extent:?}");
            assert!(extent.cy > 0 && extent.cy <= 14, "{percent}: {extent:?}");
        }
        let extent = surface.draw_text(&[0x2014], 16).unwrap();
        assert!(extent.cx > 0 && extent.cx <= 14 && extent.cy > 0 && extent.cy <= 14);
    }

    #[test]
    fn rejects_out_of_contract_values_and_unbounded_sizes() {
        for percent in [101, 255] {
            assert!(icon(Some(percent)).is_err());
        }
        for size in [0, 15, 33, u32::MAX] {
            assert!(render(Some(67), size).is_err());
        }
    }

    #[test]
    #[ignore = "set OPENMEMDUCT_TRAY_VALIDATION_DIR to write raw RGBA fixtures"]
    fn write_validation_rgba() {
        let _guard = GDI_TEST.lock().unwrap();
        let directory = std::path::PathBuf::from(
            std::env::var_os("OPENMEMDUCT_TRAY_VALIDATION_DIR").expect("validation directory"),
        );
        assert!(directory.is_absolute());
        std::fs::create_dir_all(&directory).unwrap();
        for size in [16, 20, 24, 32] {
            for percent in [Some(0), Some(9), Some(67), Some(99), Some(100), None] {
                let name = percent
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "unknown".into());
                let image = render(percent, size).unwrap();
                std::fs::write(
                    directory.join(format!("tray-{name}-{size}.rgba")),
                    image.rgba(),
                )
                .unwrap();
            }
        }
    }
}
