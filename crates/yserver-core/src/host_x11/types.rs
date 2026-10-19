//! Host input/window metadata shared with the embedded core.
use std::collections::HashMap;
use yserver_protocol::x11::ResourceId;
#[derive(Clone, Copy, Debug)]
pub enum HostEvent {
    Key(HostKeyEvent),
    Pointer(HostPointerEvent),
    Expose(HostExposeEvent),
    Configure(HostConfigureEvent),
    Closed,
}

#[derive(Clone, Copy, Debug)]
pub struct HostExposeEvent {
    pub host_xid: u32,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub count: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct HostConfigureEvent {
    pub host_xid: u32,
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEventKind {
    ButtonPress,
    ButtonRelease,
    MotionNotify,
    EnterNotify,
    LeaveNotify,
}

#[derive(Clone, Copy, Debug)]
pub struct HostPointerEvent {
    /// Producer identity for source-aware XI2 fanout.
    pub origin: crate::core_loop::message::InputOrigin,
    pub kind: PointerEventKind,
    pub host_xid: u32,
    pub detail: u8,
    pub time: u32,
    pub root_x: i16,
    pub root_y: i16,
    pub event_x: i16,
    pub event_y: i16,
    pub state: u16,
    /// X11 crossing mode: 0=NotifyNormal, 1=NotifyGrab, 2=NotifyUngrab.
    /// Only meaningful when `kind` is `EnterNotify`/`LeaveNotify`.
    pub crossing_mode: u8,
    /// X11 EnterNotify/LeaveNotify `child` field, as computed by the
    /// producer per X11 spec (see `crossings::CrossingEvent::child`).
    /// 0 = X11 `None`. The encoder layer uses this directly for
    /// Crossing events rather than the propagation walk's child,
    /// since spec-correct child semantics aren't a function of the
    /// fanout walk.
    pub child: u32,
    /// Raw relative device delta for a `MotionNotify` (libinput dx/dy,
    /// pre-clamp; summed across coalesced motions). Emitted as the XI2
    /// RawMotion valuator so relative-mouse apps (SDL2 games) get true
    /// deltas instead of the absolute position. 0 for non-motion events.
    /// (#96 follow-up: chromium-bsu ship-pins-to-corner bug)
    pub raw_dx: i32,
    pub raw_dy: i32,
    /// A crossing from a window-tree change under a still pointer, not from
    /// device motion. Its XI2 form names the master pointer as `sourceid`,
    /// as Xorg's `CheckMotion(NULL)` does (`dix/events.c:3244`).
    pub tree_change: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct HostKeyEvent {
    pub origin: crate::core_loop::message::InputOrigin,
    pub pressed: bool,
    pub keycode: u8,
    pub time: u32,
    pub root_x: i16,
    pub root_y: i16,
    pub event_x: i16,
    pub event_y: i16,
    pub state: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct PointerPosition {
    pub same_screen: bool,
    pub win_x: i16,
    pub win_y: i16,
    pub mask: u16,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostSubwindowConfig {
    pub x: Option<i16>,
    pub y: Option<i16>,
    pub width: Option<u16>,
    pub height: Option<u16>,
    pub border_width: Option<u16>,
    pub sibling: Option<u32>,
    pub stack_mode: Option<u8>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostSubwindowVisual {
    CopyFromParent,
    /// Preserve the nested window depth for non-host backends that do
    /// not need an upstream visual/colormap translation. Host X11
    /// request encoding treats this like CopyFromParent; KMS backends
    /// use the carried depth directly.
    DepthOnly {
        depth: u8,
    },
    Explicit {
        depth: u8,
        visual_xid: u32,
        colormap_xid: u32,
    },
}

impl HostSubwindowVisual {
    #[cfg(feature = "native")]
    pub(super) fn depth(self) -> u8 {
        match self {
            Self::CopyFromParent | Self::DepthOnly { .. } => 0,
            Self::Explicit { depth, .. } => depth,
        }
    }

    #[cfg(feature = "native")]
    pub(super) fn visual_xid(self) -> u32 {
        match self {
            Self::CopyFromParent | Self::DepthOnly { .. } => 0,
            Self::Explicit { visual_xid, .. } => visual_xid,
        }
    }
}

/// Host-XID → nested ResourceId lookup. F2 demoted this from
/// `Arc<Mutex<HashMap>>` to a plain `HashMap` — only the core thread
/// reads or writes it, so the lock is unnecessary.
pub type HostXidMap = HashMap<u32, ResourceId>;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum HostSocketStatus {
    /// Read drained the kernel buffer; nothing more to read right now.
    /// Frames decoded out of `read_buffer` have been classified.
    WouldBlock,
    /// Host closed the connection. The core's host-X11 poll-source arm
    /// posts `Message::Shutdown` in response.
    Eof,
}
pub(crate) const POINTER_EVENT_MASK: u32 = 0x0000_0004 // ButtonPress
    | 0x0000_0008 // ButtonRelease
    | 0x0000_0010 // EnterWindow
    | 0x0000_0020 // LeaveWindow
    | 0x0000_0040 // PointerMotion
    | 0x0000_8000; // Exposure

pub(crate) const SUBWINDOW_EVENT_MASK: u32 = 0x0000_8000;
