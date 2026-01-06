现在截图的窗口探测功能，是基于窗口尺寸排序的。我发现 KWin::WorkspaceWrapper 有一个这样的 API :

QList< KWin::Window * > stackingOrder: List of Clients currently managed by KWin, orderd by their visibility (later ones cover earlier ones).

改为用这个 API 获取窗口列表，然后直接用它原来的排序 reverse 一下，把我们按窗口尺寸的排序功能去掉。

---

你找一下 sorted by size 这句代码，这还有个排序，你要删掉才可以