//! arcterm-vt — VT parser and terminal state machine.

pub mod handler;
pub mod processor;

pub use handler::Handler;
pub use processor::Processor;

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
