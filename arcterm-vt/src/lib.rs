//! arcterm-vt — VT parser and terminal state machine.

pub mod handler;
pub mod processor;

pub use arcterm_core::TermModes;
pub use handler::{ContentType, GridState, Handler, StructuredContentAccumulator};
pub use processor::{ApcScanner, Processor};

#[cfg(test)]
mod handler_tests {
    use arcterm_core::{CellAttrs, Color, CursorPos, Grid, GridSize};

    use crate::Handler;

    fn make_grid(rows: usize, cols: usize) -> Grid {
        Grid::new(GridSize::new(rows, cols))
    }

    // -------------------------------------------------------------------------
    // put_char
    // -------------------------------------------------------------------------

    #[test]
    fn put_char_writes_char_and_advances_cursor() {
        let mut g = make_grid(5, 10);
        g.put_char('A');
        assert_eq!(g.cell(0, 0).c, 'A');
        assert_eq!(g.cursor(), CursorPos { row: 0, col: 1 });
    }

    #[test]
    fn put_char_at_end_of_line_wraps_to_next_row() {
        let mut g = make_grid(5, 4);
        // fill 4 cols — last put_char wraps
        g.put_char('a');
        g.put_char('b');
        g.put_char('c');
        g.put_char('d'); // fills col 3, cursor moves to (1,0)
        assert_eq!(g.cursor(), CursorPos { row: 1, col: 0 });
        g.put_char('e');
        assert_eq!(g.cell(1, 0).c, 'e');
    }

    #[test]
    fn put_char_at_bottom_right_scrolls_grid() {
        let mut g = make_grid(2, 2);
        // Write to every cell in a 2×2 grid so we reach the bottom-right corner
        g.put_char('a'); // (0,0)
        g.put_char('b'); // (0,1) -> wraps to (1,0)
        g.put_char('c'); // (1,0)
        g.put_char('d'); // (1,1) -> wraps, grid now full, next write scrolls
        // cursor is at (1,0) after wrap? Actually the last put_char wraps to (2,0) which is
        // out of bounds — should trigger a scroll, cursor at (1,0)
        g.put_char('e'); // should scroll up, then write at (1,0)
        assert_eq!(g.cell(1, 0).c, 'e');
        // After scroll, row 0 was row 1, so cell(0,*) was 'c','d'
        assert_eq!(g.cell(0, 0).c, 'c');
    }

    // -------------------------------------------------------------------------
    // newline / line_feed
    // -------------------------------------------------------------------------

    #[test]
    fn newline_from_last_row_scrolls_up() {
        let mut g = make_grid(3, 4);
        g.put_char('X');
        // move cursor to last row
        g.set_cursor(CursorPos { row: 2, col: 0 });
        g.newline();
        // grid scrolled, cursor stays at row 2 (last row)
        assert_eq!(g.cursor().row, 2);
    }

    #[test]
    fn line_feed_same_as_newline() {
        let mut g = make_grid(5, 5);
        g.set_cursor(CursorPos { row: 0, col: 0 });
        g.line_feed();
        assert_eq!(g.cursor().row, 1);
    }

    // -------------------------------------------------------------------------
    // carriage_return / backspace
    // -------------------------------------------------------------------------

    #[test]
    fn carriage_return_moves_to_col_zero() {
        let mut g = make_grid(5, 10);
        g.set_cursor(CursorPos { row: 1, col: 7 });
        g.carriage_return();
        assert_eq!(g.cursor(), CursorPos { row: 1, col: 0 });
    }

    #[test]
    fn backspace_moves_cursor_left_one() {
        let mut g = make_grid(5, 10);
        g.set_cursor(CursorPos { row: 0, col: 5 });
        g.backspace();
        assert_eq!(g.cursor(), CursorPos { row: 0, col: 4 });
    }

    #[test]
    fn backspace_at_col_zero_stays_at_col_zero() {
        let mut g = make_grid(5, 10);
        g.set_cursor(CursorPos { row: 0, col: 0 });
        g.backspace();
        assert_eq!(g.cursor(), CursorPos { row: 0, col: 0 });
    }

    // -------------------------------------------------------------------------
    // set_cursor_pos clamping
    // -------------------------------------------------------------------------

    #[test]
    fn set_cursor_pos_clamps_to_bounds() {
        let mut g = make_grid(5, 10);
        g.set_cursor_pos(100, 200);
        assert_eq!(g.cursor(), CursorPos { row: 4, col: 9 });
    }

    #[test]
    fn set_cursor_pos_normal_case() {
        let mut g = make_grid(10, 20);
        g.set_cursor_pos(3, 7);
        assert_eq!(g.cursor(), CursorPos { row: 3, col: 7 });
    }

    // -------------------------------------------------------------------------
    // erase_in_display
    // -------------------------------------------------------------------------

    #[test]
    fn erase_in_display_mode2_clears_all_cells() {
        let mut g = make_grid(3, 3);
        g.cell_mut(0, 0).set_char('Z');
        g.cell_mut(2, 2).set_char('Z');
        g.erase_in_display(2);
        for row in g.rows() {
            for cell in row {
                assert_eq!(cell.c, ' ', "all cells must be space after erase mode 2");
            }
        }
    }

    #[test]
    fn erase_in_display_mode0_clears_below_cursor() {
        let mut g = make_grid(4, 4);
        // Fill all cells with 'X'
        for r in 0..4 {
            for c in 0..4 {
                g.cell_mut(r, c).set_char('X');
            }
        }
        g.set_cursor(CursorPos { row: 2, col: 1 });
        g.erase_in_display(0); // clear from cursor to end
        // row 0 and row 1 should still be 'X'
        assert_eq!(g.cell(0, 0).c, 'X');
        assert_eq!(g.cell(1, 3).c, 'X');
        // row 2 from col 1 onward should be cleared
        assert_eq!(g.cell(2, 0).c, 'X'); // col 0 before cursor, untouched
        assert_eq!(g.cell(2, 1).c, ' ');
        assert_eq!(g.cell(3, 3).c, ' ');
    }

    // -------------------------------------------------------------------------
    // erase_in_line
    // -------------------------------------------------------------------------

    #[test]
    fn erase_in_line_mode0_clears_from_cursor_to_end() {
        let mut g = make_grid(3, 6);
        for c in 0..6 {
            g.cell_mut(1, c).set_char('X');
        }
        g.set_cursor(CursorPos { row: 1, col: 3 });
        g.erase_in_line(0);
        // cols 0-2 untouched
        assert_eq!(g.cell(1, 0).c, 'X');
        assert_eq!(g.cell(1, 2).c, 'X');
        // cols 3-5 cleared
        assert_eq!(g.cell(1, 3).c, ' ');
        assert_eq!(g.cell(1, 5).c, ' ');
    }

    #[test]
    fn erase_in_line_mode2_clears_entire_line() {
        let mut g = make_grid(3, 6);
        for c in 0..6 {
            g.cell_mut(1, c).set_char('X');
        }
        g.set_cursor(CursorPos { row: 1, col: 3 });
        g.erase_in_line(2);
        for c in 0..6 {
            assert_eq!(g.cell(1, c).c, ' ');
        }
    }

    // -------------------------------------------------------------------------
    // scroll_up / scroll_down
    // -------------------------------------------------------------------------

    #[test]
    fn scroll_up_moves_row1_to_row0_and_clears_bottom() {
        let mut g = make_grid(3, 3);
        g.cell_mut(0, 0).set_char('A');
        g.cell_mut(1, 0).set_char('B');
        g.cell_mut(2, 0).set_char('C');
        Handler::scroll_up(&mut g, 1);
        assert_eq!(g.cell(0, 0).c, 'B');
        assert_eq!(g.cell(1, 0).c, 'C');
        assert_eq!(g.cell(2, 0).c, ' ');
    }

    #[test]
    fn scroll_down_moves_row0_to_row1_and_clears_top() {
        let mut g = make_grid(3, 3);
        g.cell_mut(0, 0).set_char('A');
        g.cell_mut(1, 0).set_char('B');
        g.cell_mut(2, 0).set_char('C');
        Handler::scroll_down(&mut g, 1);
        assert_eq!(g.cell(0, 0).c, ' ');
        assert_eq!(g.cell(1, 0).c, 'A');
        assert_eq!(g.cell(2, 0).c, 'B');
    }

    // -------------------------------------------------------------------------
    // SGR parsing
    // -------------------------------------------------------------------------

    #[test]
    fn sgr_reset_clears_attrs() {
        let mut g = make_grid(5, 10);
        // Set some attrs first
        g.set_attrs(CellAttrs {
            fg: Color::Indexed(1),
            bold: true,
            ..Default::default()
        });
        g.set_sgr(&[0]);
        assert_eq!(g.current_attrs(), CellAttrs::default());
    }

    #[test]
    fn sgr_fg_color_30_to_37() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[31]); // red (index 1)
        assert_eq!(g.current_attrs().fg, Color::Indexed(1));
    }

    #[test]
    fn sgr_bold() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[1]);
        assert!(g.current_attrs().bold);
    }

    #[test]
    fn sgr_italic() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[3]);
        assert!(g.current_attrs().italic);
    }

    #[test]
    fn sgr_underline() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[4]);
        assert!(g.current_attrs().underline);
    }

    #[test]
    fn sgr_reverse() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[7]);
        assert!(g.current_attrs().reverse);
    }

    #[test]
    fn sgr_bg_color_40_to_47() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[42]); // green bg (index 2)
        assert_eq!(g.current_attrs().bg, Color::Indexed(2));
    }

    #[test]
    fn sgr_bright_fg_90_to_97() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[91]); // bright red (index 9)
        assert_eq!(g.current_attrs().fg, Color::Indexed(9));
    }

    #[test]
    fn sgr_bright_bg_100_to_107() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[102]); // bright green bg (index 10)
        assert_eq!(g.current_attrs().bg, Color::Indexed(10));
    }

    #[test]
    fn sgr_256_color_fg() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[38, 5, 196]);
        assert_eq!(g.current_attrs().fg, Color::Indexed(196));
    }

    #[test]
    fn sgr_256_color_bg() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[48, 5, 200]);
        assert_eq!(g.current_attrs().bg, Color::Indexed(200));
    }

    #[test]
    fn sgr_rgb_fg() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[38, 2, 255, 128, 0]);
        assert_eq!(g.current_attrs().fg, Color::Rgb(255, 128, 0));
    }

    #[test]
    fn sgr_rgb_bg() {
        let mut g = make_grid(5, 10);
        g.set_sgr(&[48, 2, 10, 20, 30]);
        assert_eq!(g.current_attrs().bg, Color::Rgb(10, 20, 30));
    }

    // -------------------------------------------------------------------------
    // title
    // -------------------------------------------------------------------------

    #[test]
    fn set_title_stores_title() {
        let mut g = make_grid(5, 10);
        g.set_title("MyTitle");
        assert_eq!(g.title(), Some("MyTitle"));
    }

    // -------------------------------------------------------------------------
    // tab
    // -------------------------------------------------------------------------

    #[test]
    fn tab_advances_to_next_tab_stop() {
        let mut g = make_grid(5, 80);
        g.set_cursor(CursorPos { row: 0, col: 0 });
        g.tab();
        assert_eq!(g.cursor().col, 8);
    }

    #[test]
    fn tab_from_mid_stop_advances_to_next() {
        let mut g = make_grid(5, 80);
        g.set_cursor(CursorPos { row: 0, col: 5 });
        g.tab();
        assert_eq!(g.cursor().col, 8);
    }
}

#[cfg(test)]
mod processor_tests {
    use arcterm_core::{Color, CursorPos, Grid, GridSize};

    use crate::{Handler, Processor};

    fn make_grid(rows: usize, cols: usize) -> Grid {
        Grid::new(GridSize::new(rows, cols))
    }

    fn feed(grid: &mut Grid, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(grid, bytes);
    }

    // -------------------------------------------------------------------------
    // Plain text
    // -------------------------------------------------------------------------

    #[test]
    fn feed_hello_writes_chars_at_row0() {
        let mut g = make_grid(24, 80);
        feed(&mut g, b"Hello");
        assert_eq!(g.cell(0, 0).c, 'H');
        assert_eq!(g.cell(0, 1).c, 'e');
        assert_eq!(g.cell(0, 2).c, 'l');
        assert_eq!(g.cell(0, 3).c, 'l');
        assert_eq!(g.cell(0, 4).c, 'o');
        assert_eq!(g.cursor(), CursorPos { row: 0, col: 5 });
    }

    // -------------------------------------------------------------------------
    // CSI J — Erase in Display
    // -------------------------------------------------------------------------

    #[test]
    fn esc_csi_2j_clears_all_cells() {
        let mut g = make_grid(24, 80);
        // Pre-fill some cells
        g.cell_mut(0, 0).set_char('Z');
        g.cell_mut(5, 5).set_char('Z');
        feed(&mut g, b"\x1b[2J");
        for row in g.rows() {
            for cell in row {
                assert_eq!(cell.c, ' ');
            }
        }
    }

    // -------------------------------------------------------------------------
    // CSI H — Cursor Position
    // -------------------------------------------------------------------------

    #[test]
    fn esc_csi_5_10_h_positions_cursor() {
        let mut g = make_grid(24, 80);
        // ESC[5;10H = row 5, col 10 (1-based) = row 4, col 9 (0-based)
        feed(&mut g, b"\x1b[5;10H");
        assert_eq!(g.cursor(), CursorPos { row: 4, col: 9 });
    }

    // -------------------------------------------------------------------------
    // CSI m — SGR colors
    // -------------------------------------------------------------------------

    #[test]
    fn esc_sgr_31_sets_fg_red_then_reset() {
        let mut g = make_grid(24, 80);
        // ESC[31m sets fg=Indexed(1), then "Red", then ESC[0m resets
        feed(&mut g, b"\x1b[31mRed\x1b[0m");
        // Cells with "Red" should have fg=Indexed(1)
        assert_eq!(g.cell(0, 0).attrs.fg, Color::Indexed(1));
        assert_eq!(g.cell(0, 1).attrs.fg, Color::Indexed(1));
        assert_eq!(g.cell(0, 2).attrs.fg, Color::Indexed(1));
        // After ESC[0m the current attrs should be default
        assert_eq!(g.current_attrs().fg, Color::Default);
    }

    // -------------------------------------------------------------------------
    // CSI A — Cursor Up
    // -------------------------------------------------------------------------

    #[test]
    fn esc_csi_2a_moves_cursor_up_2() {
        let mut g = make_grid(24, 80);
        // Position cursor at row 5, then move up 2
        g.set_cursor_pos(5, 0);
        feed(&mut g, b"\x1b[2A");
        assert_eq!(g.cursor().row, 3);
    }

    #[test]
    fn esc_csi_a_at_row_0_clamps_to_row_0() {
        let mut g = make_grid(24, 80);
        // cursor is at (0,0); moving up 2 should clamp to 0
        feed(&mut g, b"\x1b[2A");
        assert_eq!(g.cursor().row, 0);
    }

    // -------------------------------------------------------------------------
    // CSI K — Erase in Line
    // -------------------------------------------------------------------------

    #[test]
    fn esc_csi_k_erases_to_end_of_line() {
        let mut g = make_grid(24, 80);
        // Write "Hello" then position at col 3 and erase to end
        feed(&mut g, b"Hello");
        g.set_cursor_pos(0, 3);
        feed(&mut g, b"\x1b[K");
        assert_eq!(g.cell(0, 0).c, 'H');
        assert_eq!(g.cell(0, 1).c, 'e');
        assert_eq!(g.cell(0, 2).c, 'l');
        assert_eq!(g.cell(0, 3).c, ' ');
        assert_eq!(g.cell(0, 4).c, ' ');
    }

    // -------------------------------------------------------------------------
    // CR + LF
    // -------------------------------------------------------------------------

    #[test]
    fn cr_lf_moves_to_next_line_start() {
        let mut g = make_grid(24, 80);
        feed(&mut g, b"Hi\r\n");
        // After CR: col=0, after LF: row=1
        assert_eq!(g.cursor(), CursorPos { row: 1, col: 0 });
    }

    // -------------------------------------------------------------------------
    // Full integration — colored text followed by reset
    // -------------------------------------------------------------------------

    #[test]
    fn full_integration_colored_text() {
        let mut g = make_grid(24, 80);
        // Equivalent to: echo -e "\x1b[31mhello\x1b[0m world"
        feed(&mut g, b"\x1b[31mhello\x1b[0m world");
        // "hello" chars should have red fg
        for col in 0..5 {
            assert_eq!(
                g.cell(0, col).attrs.fg,
                Color::Indexed(1),
                "col {col} should be red"
            );
        }
        // " world" (starting at col 5) should have default fg
        for col in 5..11 {
            assert_eq!(
                g.cell(0, col).attrs.fg,
                Color::Default,
                "col {col} should be default after reset"
            );
        }
    }
}

// =============================================================================
// Edge case tests — Task 3
// =============================================================================

#[cfg(test)]
mod edge_case_tests {
    use arcterm_core::{Color, CursorPos, Grid, GridSize};

    use crate::{Handler, Processor};

    fn make_grid(rows: usize, cols: usize) -> Grid {
        Grid::new(GridSize::new(rows, cols))
    }

    fn feed(grid: &mut Grid, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(grid, bytes);
    }

    // -------------------------------------------------------------------------
    // Line wrapping: 81 chars into an 80-column grid
    // -------------------------------------------------------------------------

    #[test]
    fn line_wrapping_81_chars_in_80_col_grid() {
        let mut g = make_grid(24, 80);
        // Feed 80 'a' chars — cursor should be at (1,0) after wrap
        let row0: Vec<u8> = b"a".repeat(80);
        feed(&mut g, &row0);
        assert_eq!(
            g.cursor(),
            CursorPos { row: 1, col: 0 },
            "after 80 chars cursor should be at (1,0)"
        );
        // Feed the 81st char
        feed(&mut g, b"b");
        assert_eq!(g.cell(1, 0).c, 'b', "81st char must be at row 1, col 0");
        assert_eq!(
            g.cursor(),
            CursorPos { row: 1, col: 1 },
            "cursor must be at (1,1) after 81st char"
        );
    }

    // -------------------------------------------------------------------------
    // Scrolling: fill 24 rows, add newline
    // -------------------------------------------------------------------------

    #[test]
    fn scrolling_after_24_rows_fills_content_correctly() {
        let mut g = make_grid(24, 80);
        // Fill all 24 rows with a newline sequence (24 lines of 'X' then CR+LF)
        // After filling row 23, one more newline should scroll content up.
        for row in 0..24u8 {
            // Write a distinguishable char into each row
            g.set_cursor_pos(row as usize, 0);
            let ch = (b'A' + row % 26) as char;
            g.put_char(ch);
        }
        // cursor is at row 23 (last row) — do a newline which should scroll
        g.set_cursor_pos(23, 0);
        feed(&mut g, b"\n"); // LF triggers line_feed
        // After scrolling: what was row 1 is now row 0
        // Original row 0 had 'A', row 1 had 'B'
        assert_eq!(
            g.cell(0, 0).c, 'B',
            "after scroll, row 0 should contain what was row 1"
        );
        // Row 23 (last) should be blank (new blank row from scroll)
        assert_eq!(g.cell(23, 0).c, ' ', "after scroll, last row should be blank");
    }

    // -------------------------------------------------------------------------
    // Tab stops (every 8 cols)
    // -------------------------------------------------------------------------

    #[test]
    fn tab_stop_places_char_at_col_8() {
        let mut g = make_grid(5, 80);
        // Feed \t then X — X should land at column 8
        feed(&mut g, b"\tX");
        assert_eq!(g.cell(0, 8).c, 'X', "X must be at column 8 after a tab");
    }

    #[test]
    fn tab_from_col_4_places_char_at_col_8() {
        let mut g = make_grid(5, 80);
        // 4 spaces then tab then 'Y' — Y should be at col 8
        feed(&mut g, b"    \tY");
        assert_eq!(g.cell(0, 8).c, 'Y', "Y must be at col 8 after tab from col 4");
    }

    // -------------------------------------------------------------------------
    // 256-color SGR (38;5;196)
    // -------------------------------------------------------------------------

    #[test]
    fn sgr_256_color_fg_via_processor() {
        let mut g = make_grid(5, 20);
        feed(&mut g, b"\x1b[38;5;196mX");
        assert_eq!(
            g.cell(0, 0).attrs.fg,
            Color::Indexed(196),
            "cell must have fg=Indexed(196)"
        );
    }

    // -------------------------------------------------------------------------
    // RGB color SGR (38;2;255;128;0)
    // -------------------------------------------------------------------------

    #[test]
    fn sgr_rgb_color_fg_via_processor() {
        let mut g = make_grid(5, 20);
        feed(&mut g, b"\x1b[38;2;255;128;0mX");
        assert_eq!(
            g.cell(0, 0).attrs.fg,
            Color::Rgb(255, 128, 0),
            "cell must have fg=Rgb(255,128,0)"
        );
    }

    // -------------------------------------------------------------------------
    // Multi-param SGR: 1;31;42m → bold, fg=red(1), bg=green(2)
    // -------------------------------------------------------------------------

    #[test]
    fn sgr_multi_param_bold_fg_bg() {
        let mut g = make_grid(5, 20);
        feed(&mut g, b"\x1b[1;31;42mX");
        let attrs = g.cell(0, 0).attrs;
        assert!(attrs.bold, "bold must be set");
        assert_eq!(attrs.fg, Color::Indexed(1), "fg must be Indexed(1) = red");
        assert_eq!(attrs.bg, Color::Indexed(2), "bg must be Indexed(2) = green");
    }

    // -------------------------------------------------------------------------
    // CUP with no params (ESC[H) → cursor home (0,0)
    // -------------------------------------------------------------------------

    #[test]
    fn cup_no_params_positions_cursor_at_home() {
        let mut g = make_grid(24, 80);
        g.set_cursor_pos(10, 20);
        feed(&mut g, b"\x1b[H");
        assert_eq!(
            g.cursor(),
            CursorPos { row: 0, col: 0 },
            "ESC[H must move cursor to (0,0)"
        );
    }

    // -------------------------------------------------------------------------
    // Erase below cursor (ESC[J at row 10)
    // -------------------------------------------------------------------------

    #[test]
    fn erase_below_cursor_at_row_10() {
        let mut g = make_grid(24, 80);
        // Fill all cells with 'X'
        for r in 0..24 {
            for c in 0..80 {
                g.cell_mut(r, c).set_char('X');
            }
        }
        // Position cursor at row 10, col 0 and erase below
        feed(&mut g, b"\x1b[11;1H\x1b[J");
        // Rows 0-9 should still have 'X'
        for r in 0..10 {
            assert_eq!(g.cell(r, 0).c, 'X', "row {r} should be untouched");
        }
        // Row 10 from col 0 onward should be cleared
        for r in 10..24 {
            assert_eq!(g.cell(r, 0).c, ' ', "row {r} col 0 should be cleared");
        }
    }

    // -------------------------------------------------------------------------
    // Backspace: cursor at (0,5), BS → cursor at (0,4)
    // -------------------------------------------------------------------------

    #[test]
    fn backspace_moves_cursor_left_via_processor() {
        let mut g = make_grid(5, 20);
        g.set_cursor_pos(0, 5);
        feed(&mut g, b"\x08"); // 0x08 = BS
        assert_eq!(
            g.cursor(),
            CursorPos { row: 0, col: 4 },
            "BS must move cursor left one column"
        );
    }

    #[test]
    fn backspace_at_col_zero_does_not_go_negative() {
        let mut g = make_grid(5, 20);
        g.set_cursor_pos(0, 0);
        feed(&mut g, b"\x08");
        assert_eq!(g.cursor().col, 0, "BS at col 0 must stay at col 0");
    }
}

// =============================================================================
// Phase 2 processor tests — DEC private modes, extended CSI/ESC (Task 2 TDD)
// =============================================================================

#[cfg(test)]
mod phase2_processor_tests {
    use arcterm_core::{CursorPos, Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs(rows: usize, cols: usize) -> GridState {
        GridState::new(Grid::new(GridSize::new(rows, cols)))
    }

    fn feed_gs(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    // -------------------------------------------------------------------------
    // DEC private mode set: ESC[?25h → cursor visible
    // -------------------------------------------------------------------------

    #[test]
    fn dec_private_mode_set_25_cursor_visible() {
        let mut gs = make_gs(24, 80);
        gs.modes.cursor_visible = false;
        feed_gs(&mut gs, b"\x1b[?25h");
        assert!(gs.modes.cursor_visible, "mode 25h must make cursor visible");
    }

    #[test]
    fn dec_private_mode_reset_25_cursor_hidden() {
        let mut gs = make_gs(24, 80);
        feed_gs(&mut gs, b"\x1b[?25l");
        assert!(!gs.modes.cursor_visible, "mode 25l must hide cursor");
    }

    // -------------------------------------------------------------------------
    // DEC private mode set: ESC[?1h → app cursor keys
    // -------------------------------------------------------------------------

    #[test]
    fn dec_private_mode_set_1_app_cursor_keys() {
        let mut gs = make_gs(24, 80);
        feed_gs(&mut gs, b"\x1b[?1h");
        assert!(gs.modes.app_cursor_keys, "mode 1h must set app cursor keys");
    }

    #[test]
    fn dec_private_mode_reset_1_normal_cursor_keys() {
        let mut gs = make_gs(24, 80);
        gs.modes.app_cursor_keys = true;
        feed_gs(&mut gs, b"\x1b[?1l");
        assert!(!gs.modes.app_cursor_keys, "mode 1l must clear app cursor keys");
    }

    // -------------------------------------------------------------------------
    // DECSTBM: ESC[5;20r → scroll region rows 4..19 (0-indexed)
    // -------------------------------------------------------------------------

    #[test]
    fn decstbm_sets_scroll_region() {
        let mut gs = make_gs(24, 80);
        feed_gs(&mut gs, b"\x1b[5;20r");
        assert_eq!(gs.scroll_top, 4, "scroll_top must be 4 (1-based 5 → 0-based 4)");
        assert_eq!(gs.scroll_bottom, 19, "scroll_bottom must be 19 (1-based 20 → 0-based 19)");
        // DECSTBM moves cursor to home
        assert_eq!(gs.grid.cursor(), CursorPos { row: 0, col: 0 });
    }

    #[test]
    fn decstbm_no_params_resets_to_full_screen() {
        let mut gs = make_gs(24, 80);
        // First set a restricted region
        feed_gs(&mut gs, b"\x1b[5;20r");
        // Then reset with no params (ESC[r or ESC[1;24r)
        feed_gs(&mut gs, b"\x1b[r");
        assert_eq!(gs.scroll_top, 0);
        assert_eq!(gs.scroll_bottom, 23);
    }

    // -------------------------------------------------------------------------
    // ESC 7 / ESC 8: save and restore cursor position
    // -------------------------------------------------------------------------

    #[test]
    fn esc_7_saves_cursor_position() {
        let mut gs = make_gs(24, 80);
        gs.grid.set_cursor(CursorPos { row: 5, col: 10 });
        feed_gs(&mut gs, b"\x1b7");
        assert_eq!(gs.saved_cursor, Some(CursorPos { row: 5, col: 10 }));
    }

    #[test]
    fn esc_8_restores_cursor_position() {
        let mut gs = make_gs(24, 80);
        gs.grid.set_cursor(CursorPos { row: 5, col: 10 });
        feed_gs(&mut gs, b"\x1b7");
        gs.grid.set_cursor(CursorPos { row: 0, col: 0 });
        feed_gs(&mut gs, b"\x1b8");
        assert_eq!(gs.grid.cursor(), CursorPos { row: 5, col: 10 });
    }

    // -------------------------------------------------------------------------
    // DCH: ESC[3P → delete 3 characters at cursor
    // -------------------------------------------------------------------------

    #[test]
    fn csi_dch_deletes_chars_at_cursor() {
        let mut gs = make_gs(5, 10);
        // Write "ABCDEFGHIJ" in row 0
        for (i, c) in b"ABCDEFGHIJ".iter().enumerate() {
            gs.grid.cells[0][i].c = *c as char;
        }
        gs.grid.set_cursor(CursorPos { row: 0, col: 2 });
        feed_gs(&mut gs, b"\x1b[3P");
        // After deleting 3 chars at col 2: "AB" + "FGHIJ" + 3 spaces
        assert_eq!(gs.grid.cells[0][0].c, 'A');
        assert_eq!(gs.grid.cells[0][1].c, 'B');
        assert_eq!(gs.grid.cells[0][2].c, 'F');
        assert_eq!(gs.grid.cells[0][3].c, 'G');
        assert_eq!(gs.grid.cells[0][7].c, ' ', "vacated cells must be space");
    }

    // -------------------------------------------------------------------------
    // ICH: ESC[2@ → insert 2 blank chars at cursor
    // -------------------------------------------------------------------------

    #[test]
    fn csi_ich_inserts_blank_chars_at_cursor() {
        let mut gs = make_gs(5, 10);
        for (i, c) in b"ABCDEFGHIJ".iter().enumerate() {
            gs.grid.cells[0][i].c = *c as char;
        }
        gs.grid.set_cursor(CursorPos { row: 0, col: 2 });
        feed_gs(&mut gs, b"\x1b[2@");
        // After inserting 2 blank chars at col 2: "AB  CDEFGH" (IJ dropped)
        assert_eq!(gs.grid.cells[0][0].c, 'A');
        assert_eq!(gs.grid.cells[0][1].c, 'B');
        assert_eq!(gs.grid.cells[0][2].c, ' ', "inserted blank");
        assert_eq!(gs.grid.cells[0][3].c, ' ', "inserted blank");
        assert_eq!(gs.grid.cells[0][4].c, 'C');
        assert_eq!(gs.grid.cells[0][5].c, 'D');
    }

    // -------------------------------------------------------------------------
    // CHA: ESC[5G → cursor to column 4 (0-indexed)
    // -------------------------------------------------------------------------

    #[test]
    fn csi_cha_moves_cursor_to_column() {
        let mut gs = make_gs(24, 80);
        gs.grid.set_cursor(CursorPos { row: 3, col: 0 });
        feed_gs(&mut gs, b"\x1b[5G");
        assert_eq!(gs.grid.cursor(), CursorPos { row: 3, col: 4 });
    }

    // -------------------------------------------------------------------------
    // ESC = / ESC > keypad mode
    // -------------------------------------------------------------------------

    #[test]
    fn esc_equals_sets_app_keypad_mode() {
        let mut gs = make_gs(24, 80);
        feed_gs(&mut gs, b"\x1b=");
        assert!(gs.modes.app_keypad, "ESC= must set app keypad mode");
    }

    #[test]
    fn esc_greater_clears_app_keypad_mode() {
        let mut gs = make_gs(24, 80);
        gs.modes.app_keypad = true;
        feed_gs(&mut gs, b"\x1b>");
        assert!(!gs.modes.app_keypad, "ESC> must clear app keypad mode");
    }
}

// =============================================================================
// Phase 2 integration tests — vim/htop scenarios (Task 3 TDD)
// =============================================================================

#[cfg(test)]
mod phase2_integration_tests {
    use arcterm_core::{CursorPos, Grid, GridSize};

    use crate::{GridState, Processor};

    fn make_gs(rows: usize, cols: usize) -> GridState {
        GridState::new(Grid::new(GridSize::new(rows, cols)))
    }

    fn feed_gs(gs: &mut GridState, bytes: &[u8]) {
        let mut proc = Processor::new();
        proc.advance(gs, bytes);
    }

    // -------------------------------------------------------------------------
    // vim startup sequence:
    //   ESC[?1049h  → enter alt screen
    //   ESC[2J      → clear entire display
    //   ESC[1;1H    → cursor home
    //   ESC[1;24r   → set scroll region rows 0..23
    //   (text)
    // -------------------------------------------------------------------------

    #[test]
    fn vim_startup_enters_alt_screen_and_sets_scroll_region() {
        let mut gs = make_gs(24, 80);
        // Pre-populate the normal screen so we can verify it's saved.
        gs.grid.cells[0][0].c = 'N';

        // Enter alt screen, clear, home cursor, set scroll region (DECSTBM moves cursor home).
        feed_gs(&mut gs, b"\x1b[?1049h\x1b[2J\x1b[1;1H\x1b[1;24r");

        // After DECSTBM cursor must be at (0,0).
        assert_eq!(gs.grid.cursor(), CursorPos { row: 0, col: 0 },
            "DECSTBM must move cursor to home");

        // Now write text.
        feed_gs(&mut gs, b"Hello");

        // Alt screen should be active.
        assert!(gs.modes.alt_screen, "alt screen must be active after ESC[?1049h");
        // Normal screen must have been saved — normal_screen is Some.
        assert!(gs.normal_screen.is_some(), "normal screen must be saved");
        // Normal screen's cell (0,0) must contain the pre-populated 'N'.
        assert_eq!(
            gs.normal_screen.as_ref().unwrap().cells[0][0].c, 'N',
            "saved normal screen must preserve original content"
        );
        // Scroll region set by ESC[1;24r → top=0, bottom=23.
        assert_eq!(gs.scroll_top, 0);
        assert_eq!(gs.scroll_bottom, 23);
        // "Hello" must appear at row 0, col 0 through 4.
        assert_eq!(gs.grid.cells[0][0].c, 'H');
        assert_eq!(gs.grid.cells[0][4].c, 'o');
        // Cursor after "Hello" (5 chars) must be at (0,5).
        assert_eq!(gs.grid.cursor(), CursorPos { row: 0, col: 5 });
    }

    // -------------------------------------------------------------------------
    // vim exit sequence:
    //   ESC[r       → reset scroll region to full screen
    //   ESC[?1049l  → leave alt screen, restore normal screen
    // -------------------------------------------------------------------------

    #[test]
    fn vim_exit_restores_normal_screen() {
        let mut gs = make_gs(24, 80);
        // Simulate being in the alt screen with content.
        gs.grid.cells[5][5].c = 'A'; // Alt screen content.
        feed_gs(&mut gs, b"\x1b[?1049h"); // Enter alt screen (saves current).

        // Write some content on the alt screen.
        gs.grid.cells[3][3].c = 'Z';

        // Exit: reset scroll region then leave alt screen.
        feed_gs(&mut gs, b"\x1b[r\x1b[?1049l");

        // Normal screen restored; alt screen should not be active.
        assert!(!gs.modes.alt_screen, "alt screen must be inactive after ESC[?1049l");
        // The restored grid must have the pre-alt-screen content ('A' at (5,5)).
        assert_eq!(gs.grid.cells[5][5].c, 'A', "normal screen content must be restored");
        // Scroll region must be reset to full screen.
        assert_eq!(gs.scroll_top, 0);
        assert_eq!(gs.scroll_bottom, 23);
    }

    // -------------------------------------------------------------------------
    // htop status bar: scroll region excludes last row, scrolling only in region
    // -------------------------------------------------------------------------

    #[test]
    fn htop_scroll_region_excludes_last_row() {
        let mut gs = make_gs(24, 80);
        // Set scroll region to rows 0..22 (lines 1-23, 1-based), leaving row 23 (last) outside.
        feed_gs(&mut gs, b"\x1b[1;23r");
        assert_eq!(gs.scroll_top, 0);
        assert_eq!(gs.scroll_bottom, 22, "scroll region bottom must be row 22 (0-indexed)");

        // Fill rows 0..22 with 'R', row 23 with 'S'.
        for row in 0..23usize {
            for col in 0..80usize {
                gs.grid.cells[row][col].c = 'R';
            }
        }
        for col in 0..80usize {
            gs.grid.cells[23][col].c = 'S';
        }

        // Position cursor at the bottom of the scroll region (row 22) and send LF.
        gs.grid.set_cursor(CursorPos { row: 22, col: 0 });
        feed_gs(&mut gs, b"\n");

        // Row 23 (the status bar, outside the scroll region) must be untouched.
        assert_eq!(gs.grid.cells[23][0].c, 'S', "row 23 (status bar) must not be scrolled");
        // Row 22 must now be blank (new row scrolled in at the bottom of region).
        assert_eq!(gs.grid.cells[22][0].c, ' ', "new blank row at scroll region bottom");
        // Row 0 must have been scrolled out — content of original row 1 is now at row 0.
        assert_eq!(gs.grid.cells[0][0].c, 'R', "rows 0-21 must still be 'R' after scroll");
    }

    // -------------------------------------------------------------------------
    // Multi-mode set: ESC[?1;25;2004h → multiple modes set at once
    // -------------------------------------------------------------------------

    #[test]
    fn multi_mode_set_sets_all_modes() {
        let mut gs = make_gs(24, 80);
        // Start with all modes off.
        gs.modes.app_cursor_keys = false;
        gs.modes.cursor_visible = false;
        gs.modes.bracketed_paste = false;

        feed_gs(&mut gs, b"\x1b[?1;25;2004h");

        assert!(gs.modes.app_cursor_keys, "mode 1 must be set");
        assert!(gs.modes.cursor_visible, "mode 25 must be set");
        assert!(gs.modes.bracketed_paste, "mode 2004 must be set");
    }

    // -------------------------------------------------------------------------
    // Cursor above scroll region: newlines move cursor freely toward the region
    // without triggering a scroll.
    // -------------------------------------------------------------------------

    #[test]
    fn newline_above_scroll_region_moves_cursor_without_scrolling() {
        let mut gs = make_gs(10, 5);
        // Set scroll region to rows 4..7 (1-based: ESC[5;8r).
        feed_gs(&mut gs, b"\x1b[5;8r");
        assert_eq!(gs.scroll_top, 4);
        assert_eq!(gs.scroll_bottom, 7);

        // Mark the scroll region top row so we can confirm it was not scrolled.
        for col in 0..5usize {
            gs.grid.cells[4][col].c = 'T'; // 'T' for top of region
        }

        // Position cursor two rows above the scroll region (row 2) and send two LFs.
        gs.grid.set_cursor(CursorPos { row: 2, col: 0 });
        feed_gs(&mut gs, b"\n"); // row 2 -> row 3 (still above region)
        assert_eq!(
            gs.grid.cursor().row, 3,
            "first LF: cursor must move from row 2 to row 3"
        );
        // Region must not have scrolled — row 4 still contains 'T'.
        assert_eq!(gs.grid.cells[4][0].c, 'T', "scroll region must not scroll on LF above it");

        feed_gs(&mut gs, b"\n"); // row 3 -> row 4 (enters the region, still no scroll)
        assert_eq!(
            gs.grid.cursor().row, 4,
            "second LF: cursor must move from row 3 to row 4 (region top)"
        );
        // Row 4 still has 'T' — entering the region does not scroll.
        assert_eq!(gs.grid.cells[4][0].c, 'T', "region must not scroll when cursor enters it");
    }

    // -------------------------------------------------------------------------
    // Scroll region respects region boundaries during scroll_up
    // -------------------------------------------------------------------------

    #[test]
    fn scroll_up_respects_scroll_region_boundaries() {
        let mut gs = make_gs(10, 5);
        // Set scroll region to rows 2..5 (0-indexed).
        feed_gs(&mut gs, b"\x1b[3;6r"); // 1-based: rows 3 to 6

        // Fill all cells distinctly.
        for row in 0..10usize {
            for col in 0..5usize {
                gs.grid.cells[row][col].c = (b'0' + row as u8) as char;
            }
        }

        // Scroll region up by 1 (CSI 1 S).
        feed_gs(&mut gs, b"\x1b[1S");

        // Rows outside the scroll region (0, 1, 6..9) must be untouched.
        assert_eq!(gs.grid.cells[0][0].c, '0', "row 0 outside region must be unchanged");
        assert_eq!(gs.grid.cells[1][0].c, '1', "row 1 outside region must be unchanged");
        assert_eq!(gs.grid.cells[6][0].c, '6', "row 6 outside region must be unchanged");

        // Inside the region (rows 2..5): row 2 should now have what was row 3.
        assert_eq!(gs.grid.cells[2][0].c, '3', "row 2 should be former row 3 after scroll up");
        // Row 5 (bottom of region) should be blank.
        assert_eq!(gs.grid.cells[5][0].c, ' ', "row 5 (region bottom) should be blank");
    }
}

// =============================================================================
// Phase 4 ApcScanner tests (Task 1 TDD) — written before implementation
// =============================================================================

#[cfg(test)]
mod apc_scanner_tests {
    use crate::{ApcScanner, Handler};

    /// Minimal handler that records kitty_graphics_command payloads.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<Vec<u8>>,
        chars: Vec<char>,
    }

    impl Handler for Recorder {
        fn put_char(&mut self, c: char) {
            self.chars.push(c);
        }
        fn kitty_graphics_command(&mut self, payload: &[u8]) {
            self.calls.push(payload.to_vec());
        }
    }

    /// Helper: feed bytes through ApcScanner wrapping a Recorder, return recorder.
    fn scan(input: &[u8]) -> Recorder {
        let mut rec = Recorder::default();
        let mut scanner = ApcScanner::new();
        scanner.advance(&mut rec, input);
        rec
    }

    // -------------------------------------------------------------------------
    // Complete APC sequence: ESC _ payload ESC \
    // -------------------------------------------------------------------------

    #[test]
    fn complete_apc_sequence_dispatches_payload() {
        let input = b"\x1b_Ga=q;\x1b\\";
        let rec = scan(input);
        assert_eq!(rec.calls.len(), 1, "one kitty_graphics_command must fire");
        assert_eq!(rec.calls[0], b"Ga=q;", "payload must be stripped of APC delimiters");
        assert!(rec.chars.is_empty(), "no plain chars must be emitted");
    }

    // -------------------------------------------------------------------------
    // Split at ESC boundary: two calls together reconstruct one APC
    // -------------------------------------------------------------------------

    #[test]
    fn split_at_esc_boundary_reconstructs_apc() {
        // Split just before the trailing ESC \
        let part1 = b"\x1b_hello";
        let part2 = b"\x1b\\world";
        let mut rec = Recorder::default();
        let mut scanner = ApcScanner::new();
        scanner.advance(&mut rec, part1);
        scanner.advance(&mut rec, part2);
        // "hello" was the payload; "world" is plain text after ST
        assert_eq!(rec.calls.len(), 1);
        assert_eq!(rec.calls[0], b"hello");
        assert_eq!(rec.chars, vec!['w', 'o', 'r', 'l', 'd']);
    }

    // -------------------------------------------------------------------------
    // Split at ST boundary: ESC arrives alone, \ in next call
    // -------------------------------------------------------------------------

    #[test]
    fn split_at_st_boundary_reconstructs_apc() {
        let part1 = b"\x1b_payload\x1b";
        let part2 = b"\\";
        let mut rec = Recorder::default();
        let mut scanner = ApcScanner::new();
        scanner.advance(&mut rec, part1);
        scanner.advance(&mut rec, part2);
        assert_eq!(rec.calls.len(), 1);
        assert_eq!(rec.calls[0], b"payload");
        assert!(rec.chars.is_empty());
    }

    // -------------------------------------------------------------------------
    // Non-APC input forwarded as plain chars
    // -------------------------------------------------------------------------

    #[test]
    fn non_apc_input_forwarded_as_plain_chars() {
        let rec = scan(b"hello");
        assert!(rec.calls.is_empty(), "no APC dispatch for plain text");
        assert_eq!(rec.chars, vec!['h', 'e', 'l', 'l', 'o']);
    }

    // -------------------------------------------------------------------------
    // ESC followed by non-underscore is forwarded, not treated as APC start
    // -------------------------------------------------------------------------

    #[test]
    fn esc_non_underscore_forwarded_not_apc() {
        // ESC[31m (CSI sequence) — the underlying Processor handles it; the
        // ApcScanner must not swallow the ESC.  In ApcScanner the ESC is
        // forwarded to the inner Processor, which dispatches it normally.
        let rec = scan(b"AB");
        assert!(rec.calls.is_empty());
        assert_eq!(rec.chars, vec!['A', 'B']);
    }

    // -------------------------------------------------------------------------
    // Empty APC: ESC _ ESC \  → payload is empty slice
    // -------------------------------------------------------------------------

    #[test]
    fn empty_apc_payload_dispatches_empty_slice() {
        let input = b"\x1b_\x1b\\";
        let rec = scan(input);
        assert_eq!(rec.calls.len(), 1, "empty APC must still dispatch");
        assert!(rec.calls[0].is_empty(), "payload must be empty");
    }

    // -------------------------------------------------------------------------
    // Large payload (performance batch path)
    // -------------------------------------------------------------------------

    #[test]
    fn large_payload_dispatched_completely() {
        let payload: Vec<u8> = (0u8..=127).cycle().take(4096).collect();
        let mut input = vec![0x1b, b'_'];
        input.extend_from_slice(&payload);
        input.push(0x1b);
        input.push(b'\\');
        let rec = scan(&input);
        assert_eq!(rec.calls.len(), 1);
        assert_eq!(rec.calls[0], payload);
    }

    // -------------------------------------------------------------------------
    // Multiple APC sequences in one buffer
    // -------------------------------------------------------------------------

    #[test]
    fn multiple_apc_sequences_in_one_buffer() {
        let input = b"\x1b_first\x1b\\\x1b_second\x1b\\";
        let rec = scan(input);
        assert_eq!(rec.calls.len(), 2);
        assert_eq!(rec.calls[0], b"first");
        assert_eq!(rec.calls[1], b"second");
    }
}
