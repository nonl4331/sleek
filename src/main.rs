use std::ffi::CString;
use std::mem::MaybeUninit;
use std::ops::Drop;
use x11::xlib::*;

const LINE_WIDTH: i32 = 5;
const REFRESH_RATE: u32 = 60;
const MIN_TIME_BETWEEN_UPDATES: u64 = ((0.5 / REFRESH_RATE as f64) * 1000000000.0) as u64;
const LINE_COLOUR: RGB = RGB::new(128, 0, 128);

const XNONE: u64 = 0;

#[derive(Copy, Clone, Debug)]
struct RGB {
    r: u8,
    g: u8,
    b: u8,
}

impl RGB {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
}

impl Into<u64> for RGB {
    fn into(self) -> u64 {
        (self.r as u64) << 16 | (self.g as u64) << 8 | (self.b as u64)
    }
}

enum SelectionState {
    NotCreated,
    Selecting,
    Selected,
}

#[derive(Copy, Clone, Debug)]
struct Point {
    x: i32,
    y: i32,
}

impl Point {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn min(&self, other: &Point) -> Self {
        Self {
            x: self.x.min(other.x),
            y: self.y.min(other.y),
        }
    }
    pub fn max(&self, other: &Point) -> Self {
        Self {
            x: self.x.max(other.x),
            y: self.y.max(other.y),
        }
    }
}

struct RenderWindow {
    display: *mut _XDisplay,
    window: u64,
    graphics_context: *mut _XGC,
}

impl RenderWindow {
    pub fn new(display: *mut _XDisplay, window: u64, graphics_context: *mut _XGC) -> Self {
        Self {
            display,
            window,
            graphics_context,
        }
    }
}

impl Drop for RenderWindow {
    fn drop(&mut self) {
        unsafe {
            XFreeGC(self.display, self.graphics_context);
            XDestroyWindow(self.display, self.window);
            XCloseDisplay(self.display);
        }
    }
}

fn init_x() -> RenderWindow {
    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            panic!("XOpenDisplay failed");
        }

        let screen_number = XDefaultScreen(display);

        let mut window_attributes = XSetWindowAttributes {
            background_pixmap: XNONE,
            background_pixel: 0,
            border_pixmap: CopyFromParent as u64,
            border_pixel: 0,
            bit_gravity: ForgetGravity,
            win_gravity: NorthWestGravity,
            backing_store: NotUseful,
            backing_planes: u64::MAX,
            backing_pixel: 0,
            save_under: 0,
            event_mask: 0,
            do_not_propagate_mask: 0,
            override_redirect: 1,
            colormap: CopyFromParent as u64,
            cursor: XNONE,
        };

        let mut x = 0;
        let mut y = 0;
        let mut width = 0;
        let mut height = 0;
        let mut border_width = 0;
        let mut depth = 0;

        let mut root = XDefaultRootWindow(display);

        if XGetGeometry(
            display,
            root,
            &mut root,
            &mut x,
            &mut y,
            &mut width,
            &mut height,
            &mut border_width,
            &mut depth,
        ) == BadDrawable as i32
        {
            panic!("XGetGeometry returned BadDrawable");
        }

        let mut visual_info: XVisualInfo = MaybeUninit::zeroed().assume_init();
        if XMatchVisualInfo(
            display,
            screen_number,
            depth as i32,
            TrueColor,
            &mut visual_info,
        ) == 0
        {
            panic!("No Visual Info with 32bit true color!");
        }

        let window = XCreateWindow(
            display,
            root,
            x,
            y,
            width,
            height,
            border_width,
            depth as i32,
            CopyFromParent as u32,
            visual_info.visual,
            CWOverrideRedirect,
            &mut window_attributes,
        );

        let pixmap = XCreatePixmap(display, window, width, height, depth);

        let window_name = CString::new("sleek").unwrap();
        let icon_name = CString::new("icon").unwrap();

        XSetStandardProperties(
            display,
            window,
            window_name.as_ptr(),
            icon_name.as_ptr(),
            0,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );

        XSelectInput(
            display,
            window,
            (ButtonPressMask | KeyPressMask | ButtonReleaseMask | PointerMotionMask) as i64,
        );

        let mut gc_values = XGCValues {
            function: GXcopy,
            plane_mask: u64::MAX,
            foreground: LINE_COLOUR.into(),
            background: 0,
            line_width: LINE_WIDTH,
            line_style: LineSolid,
            cap_style: CapButt,
            join_style: JoinMiter,
            fill_style: FillSolid,
            fill_rule: EvenOddRule,
            arc_mode: ArcPieSlice,
            tile: 0,
            stipple: u64::MAX,
            ts_x_origin: 0,
            ts_y_origin: 0,
            font: 0,
            subwindow_mode: ClipByChildren,
            graphics_exposures: True,
            clip_x_origin: 0,
            clip_y_origin: 0,
            clip_mask: XNONE,
            dash_offset: 0,
            dashes: 4,
        };

        let graphics_context = XCreateGC(
            display,
            root,
            (GCLineWidth | GCForeground) as u64,
            &mut gc_values,
        );

        let image = XGetImage(display, root, x, y, width, height, XAllPlanes(), ZPixmap);

        XPutImage(
            display,
            pixmap,
            graphics_context,
            image,
            x,
            y,
            x,
            y,
            width,
            height,
        );

        XSetWindowBackgroundPixmap(display, window, pixmap);

        XMapRaised(display, window);

        XSetInputFocus(display, window, RevertToNone, CurrentTime);

        RenderWindow::new(display, window, graphics_context)
    }
}

fn main() {
    let mut render_window = init_x();
    handle_events(&mut render_window);
}

fn handle_events(render_window: &mut RenderWindow) {
    let mut point_one = Point::new(0, 0);
    let mut point_two = Point::new(0, 0);
    let mut selection = SelectionState::NotCreated;
    let mut last_update: std::time::Instant = std::time::Instant::now();

    loop {
        unsafe {
            let mut event: XEvent = std::mem::MaybeUninit::zeroed().assume_init();

            XNextEvent(render_window.display, &mut event);
            match event.type_ {
                x11::xlib::MotionNotify => {
                    if let SelectionState::Selecting = selection {
                        let x2 = event.button.x;
                        let y2 = event.button.y;
                        if x2 != point_two.x || y2 != point_two.y {
                            point_two = Point::new(x2, y2);

                            if last_update.elapsed().as_nanos() > MIN_TIME_BETWEEN_UPDATES as u128 {
                                draw_selection(render_window, point_one, point_two);
                                last_update = std::time::Instant::now();
                            }
                        }
                    }
                }
                x11::xlib::ButtonPress => {
                    if event.button.button == Button1 {
                        point_one = Point::new(event.button.x, event.button.y);
                        point_two = Point::new(event.button.x, event.button.y);
                        selection = SelectionState::Selecting;
                    }
                }
                x11::xlib::ButtonRelease => {
                    if event.button.button == Button1 {
                        point_two = Point::new(event.button.x, event.button.y);
                        draw_selection(render_window, point_one, point_two);
                        selection = SelectionState::Selected;
                    }
                }
                x11::xlib::KeyPress => {
                    if event.key.keycode == 9 {
                        //X11 ESC keycode
                        println!("Exiting");
                        return;
                    }
                }
                _ => {}
            }
        }
    }
}

fn draw_selection(render_window: &mut RenderWindow, point_one: Point, point_two: Point) {
    let min = point_one.min(&point_two);
    let max = point_one.max(&point_two);

    let width = max.x - min.x;
    let height = max.y - min.y;

    unsafe {
        XClearWindow(render_window.display, render_window.window);

        XDrawRectangle(
            render_window.display,
            render_window.window,
            render_window.graphics_context,
            min.x,
            min.y,
            width as u32,
            height as u32,
        );
    };
}
