var list = [];
var windows = workspace.stackingOrder;
// Reverse iteration: stackingOrder[0] = bottom, stackingOrder[last] = top.
// We push in reverse so list[0] = topmost window for front-to-back matching.
for (var i = windows.length - 1; i >= 0; i--) {
    var w = windows[i];
    if (w.normalWindow && !w.minimized) {
        list.push({
            caption: w.caption,
            resourceClass: w.resourceClass,
            x: w.frameGeometry.x,
            y: w.frameGeometry.y,
            width: w.frameGeometry.width,
            height: w.frameGeometry.height
        });
    }
}
callDBus("com.takeashot.service", "/com/takeashot/Service",
         "com.takeashot.Service", "receive_window_data",
         JSON.stringify(list));
