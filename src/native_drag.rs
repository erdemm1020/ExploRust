//! Starts a real macOS drag session, so rows can be dropped on Finder, other
//! apps and other windows. GPUI's own drag only lives inside the window.

use std::cell::RefCell;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSDragOperation, NSDraggingContext, NSDraggingItem, NSDraggingSession,
    NSDraggingSource, NSPasteboardWriting, NSWorkspace,
};
use objc2_foundation::{
    NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURL,
};

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ExploRustDragSource"]
    struct DragSource;

    unsafe impl NSObjectProtocol for DragSource {}

    unsafe impl NSDraggingSource for DragSource {
        #[unsafe(method(draggingSession:sourceOperationMaskForDraggingContext:))]
        fn source_operation_mask(
            &self,
            _session: &NSDraggingSession,
            _context: NSDraggingContext,
        ) -> NSDragOperation {
            NSDragOperation::Copy | NSDragOperation::Move
        }
    }
);

thread_local! {
    /// The session only keeps a weak reference to its source.
    static SOURCE: RefCell<Option<Retained<DragSource>>> = const { RefCell::new(None) };
}

/// Begin dragging `paths` out of the window, from the mouse event being handled.
/// Returns whether a session started. Must run on the main thread.
pub fn begin(paths: &[PathBuf]) -> bool {
    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    let (Some(event), Some(window)) = (app.currentEvent(), app.keyWindow()) else {
        return false;
    };
    let Some(view) = window.contentView() else {
        return false;
    };
    let at = view.convertPoint_fromView(event.locationInWindow(), None);
    let workspace = NSWorkspace::sharedWorkspace();

    let items: Vec<Retained<NSDraggingItem>> = paths
        .iter()
        .filter_map(|p| {
            let path = NSString::from_str(p.to_str()?);
            let url = NSURL::fileURLWithPath(&path);
            let writer = ProtocolObject::<dyn NSPasteboardWriting>::from_ref(&*url);
            let item = NSDraggingItem::initWithPasteboardWriter(NSDraggingItem::alloc(), writer);
            let icon = workspace.iconForFile(&path);
            let frame = NSRect::new(
                NSPoint::new(at.x - 16.0, at.y - 16.0),
                NSSize::new(32.0, 32.0),
            );
            // SAFETY: the contents are an NSImage, which dragging items accept.
            unsafe { item.setDraggingFrame_contents(frame, Some(&icon)) };
            Some(item)
        })
        .collect();
    if items.is_empty() {
        return false;
    }

    let source = SOURCE.with(|s| {
        s.borrow_mut()
            .get_or_insert_with(|| {
                let this = DragSource::alloc(mtm);
                let this = this.set_ivars(());
                // SAFETY: plain NSObject init.
                let source: Retained<DragSource> = unsafe { msg_send![super(this), init] };
                source
            })
            .clone()
    });
    let source = ProtocolObject::from_ref(&*source);
    view.beginDraggingSessionWithItems_event_source(
        &NSArray::from_retained_slice(&items),
        &event,
        source,
    );
    true
}
