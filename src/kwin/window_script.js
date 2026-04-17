var list = [];
var windows = workspace.stackingOrder;
for (var i = 0; i < windows.length; i++) {
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
