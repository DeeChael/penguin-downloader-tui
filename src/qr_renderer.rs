use penguin_downloader::provider::QrLoginData;

pub fn render_to_string(data: &QrLoginData) -> String {
    match data {
        QrLoginData::Image(bytes) => render_image(bytes),
        QrLoginData::Url(url) => render_url(url),
    }
}

const QR_VERSIONS: [u32; 40] = [
    21, 25, 29, 33, 37, 41, 45, 49, 53, 57, 61, 65, 69, 73, 77, 81, 85, 89, 93, 97, 101, 105,
    109, 113, 117, 121, 125, 129, 133, 137, 141, 145, 149, 153, 157, 161, 165, 169, 173, 177,
];

fn render_image(image_data: &[u8]) -> String {
    let img = match image::load_from_memory(image_data) {
        Ok(img) => img,
        Err(_) => return "  (\u{65e0}\u{6cd5}\u{663e}\u{793a}\u{4e8c}\u{7ef4}\u{7801})".to_string(),
    };

    let img = img.to_luma8();
    let (width, height) = img.dimensions();

    let is_black = |x: u32, y: u32| -> bool {
        if x >= width || y >= height {
            return false;
        }
        img.get_pixel(x, y)[0] < 128
    };

    let mut top = 0u32;
    't: for y in 0..height {
        for x in 0..width {
            if is_black(x, y) { top = y; break 't; }
        }
    }

    let mut bottom = height.saturating_sub(1);
    'b: for y in (0..height).rev() {
        for x in 0..width {
            if is_black(x, y) { bottom = y; break 'b; }
        }
    }

    let mut left = 0u32;
    'l: for x in 0..width {
        for y in top..=bottom {
            if is_black(x, y) { left = x; break 'l; }
        }
    }

    let mut right = width.saturating_sub(1);
    'r: for x in (0..width).rev() {
        for y in top..=bottom {
            if is_black(x, y) { right = x; break 'r; }
        }
    }

    let cw = right.saturating_sub(left) + 1;
    let ch = bottom.saturating_sub(top) + 1;

    let block_size = estimate_block_size(cw, ch);
    if block_size == 0 {
        return "  (\u{65e0}\u{6cd5}\u{663e}\u{793a}\u{4e8c}\u{7ef4}\u{7801})".to_string();
    }

    let qrw = cw / block_size;
    let qrh = ch / block_size;
    if qrw == 0 || qrh == 0 {
        return "  (\u{65e0}\u{6cd5}\u{663e}\u{793a}\u{4e8c}\u{7ef4}\u{7801})".to_string();
    }

    let row_width = qrw as usize * 2;
    let mut rows: Vec<String> = Vec::with_capacity(qrh as usize);

    for row_idx in 0..qrh {
        let mut row = String::with_capacity(row_width);
        for col_idx in 0..qrw {
            let mut black = 0u32;
            let mut total = 0u32;
            for dy in 0..block_size {
                for dx in 0..block_size {
                    let px = left + col_idx * block_size + dx;
                    let py = top + row_idx * block_size + dy;
                    if px < width && py < height {
                        total += 1;
                        if is_black(px, py) { black += 1; }
                    }
                }
            }
            if total > 0 && black * 2 > total {
                row.push_str("\u{2588}\u{2588}");
            } else {
                row.push_str("  ");
            }
        }
        rows.push(row);
    }

    rows.join("\n")
}

fn estimate_block_size(cw: u32, ch: u32) -> u32 {
    let avg = (cw + ch) / 2;
    let mut best_diff = u32::MAX;
    let mut best_bs = 1u32;

    for &vs in &QR_VERSIONS {
        if vs == 0 { continue; }
        let bs = avg / vs;
        if bs == 0 { continue; }
        let est = bs * vs;
        let diff = if est > avg { est - avg } else { avg - est };
        if diff < best_diff {
            best_diff = diff;
            best_bs = bs;
        }
    }

    best_bs.max(1)
}

fn render_url(url: &str) -> String {
    let code = match qrcode::QrCode::new(url) {
        Ok(c) => c,
        Err(_) => return format!("\u{8bf7}\u{590d}\u{5236}\u{94fe}\u{63a5}\u{5230}\u{6d4f}\u{89c8}\u{5668}\u{6253}\u{5f00}:\n{}", url),
    };

    let w = code.width();
    let colors = code.to_colors();
    let mut rows = Vec::with_capacity(w);

    for y in 0..w {
        let mut row = String::new();
        for x in 0..w {
            if colors[y * w + x] == qrcode::types::Color::Dark {
                row.push_str("\u{2588}\u{2588}");
            } else {
                row.push_str("  ");
            }
        }
        rows.push(row);
    }

    rows.join("\n")
}
