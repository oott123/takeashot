QML 图标不要自己画，改用 tabler icons 的图标库，svg 从这个目录里面找 `references/tabler-icons/icons/outline` 然后把要用到的图标 path 提取出来放到 QML 里

---

file:///home/alice/Repos/takeashot/Toolbar.qml:66:35: Cannot assign to non-existent property "d" 
                             PathSvg { d: "M7 7l10 10 M16 7l-9 0l0 9" } 
                                       ^
QML Error: file:///home/alice/Repos/takeashot/Toolbar.qml:66:35: Cannot assign to non-existent property "d"