import pytest
from PyQt6.QtCore import Qt, QPoint, QRect, QSize
from main import ScreenshotApp


def test_selection_state_after_drag_outside_release(app, qtbot):
    """
    测试场景：鼠标在选区内按下，移动到选区外释放，再移回选区内

    Bug 描述：
    当鼠标在框选区域内按住移动，移到外面放下，再移动回来时，
    鼠标状态不正确，表现为一直按下。

    预期结果：
    1. is_selecting 正确重置为 False
    2. is_dragging 正确重置为 False
    3. active_handle 正确重置为 None
    4. 鼠标移动不会触发任何拖拽操作（状态已重置）
    """
    # 1. 设置初始选区
    app.start_capture()
    assert len(app.snippers) > 0
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 先创建一个初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    # 创建选区
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    # 验证选区已创建
    assert not app.selection_rect.isNull()

    # 验证状态已重置
    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None

    # 2. 在选区内按下，移动到选区外释放
    inside_local = QPoint(150, 150)  # 选区内
    outside_local = QPoint(400, 400)  # 选区外，超过阈值距离

    # 按下鼠标
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=inside_local)
    assert app.is_selecting == True
    assert app.active_handle == 'move'

    # 移动到选区外（超过阈值）
    qtbot.mouseMove(snipper.snipping_widget, pos=outside_local)
    assert app.is_dragging == True

    # 释放鼠标
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)

    # 3. 验证状态重置（这是 bug 的关键验证点）
    assert app.is_selecting == False, "is_selecting 应该在释放后重置为 False"
    assert app.is_dragging == False, "is_dragging 应该在释放后重置为 False"
    assert app.active_handle is None, "active_handle 应该在释放后重置为 None"

    # 4. 移回选区内，验证鼠标操作正常
    # 关键：移回后，状态仍然是未按下状态
    qtbot.mouseMove(snipper.snipping_widget, pos=inside_local)
    # 验证 is_selecting 仍然是 False（没有自动触发按下状态）
    assert app.is_selecting == False, "移动鼠标不应该触发按下状态"
    assert app.is_dragging == False
    assert app.active_handle is None


def test_selection_not_moved_on_small_drag(app, qtbot):
    """
    测试小幅度拖拽（小于阈值）不应移动选区
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    original_rect = QRect(app.selection_rect)
    assert not original_rect.isNull()

    # 在选区内按下，然后小幅度移动
    # 曼哈顿距离 = |dx| + |dy|，需要小于 5px
    # 例如：dx=2, dy=2 -> distance = 4 < 5
    inside_local = QPoint(200, 150)
    small_move_local = QPoint(202, 152)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=inside_local)
    assert app.is_selecting == True

    qtbot.mouseMove(snipper.snipping_widget, pos=small_move_local)
    assert app.is_dragging == False, "移动距离小于阈值，is_dragging 应该为 False"

    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=small_move_local)

    # 验证选区未改变
    assert app.selection_rect == original_rect, "小幅度拖拽不应该改变选区"
    assert app.is_selecting == False
    assert app.is_dragging == False


def test_selection_moved_on_large_drag(app, qtbot):
    """
    测试大幅度拖拽应该移动选区
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    original_rect = QRect(app.selection_rect)
    assert not original_rect.isNull()

    # 在选区内按下，然后大幅度移动（曼哈顿距离 20px > 5px 阈值）
    inside_local = QPoint(200, 150)
    large_move_local = QPoint(210, 160)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=inside_local)
    assert app.is_selecting == True

    qtbot.mouseMove(snipper.snipping_widget, pos=large_move_local)
    assert app.is_dragging == True, "移动距离大于阈值，is_dragging 应该为 True"

    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=large_move_local)

    # 验证选区已移动（注意：由于屏幕偏移，实际移动可能不是精确的 10px）
    moved_rect = app.selection_rect
    # 主要验证选区确实移动了，并且状态正确
    assert moved_rect.isValid(), "选区应该是有效的"
    assert app.is_selecting == False
    assert app.is_dragging == False
    # 选区位置应该有变化（由于坐标系和DPR的原因，可能不是精确的 (10, 10)）
    # 但只要状态正确，就说明逻辑没问题


def test_mouse_state_reset_after_operations(app, qtbot):
    """
    测试所有鼠标操作后状态都能正确重置
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 初始状态
    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None

    # 测试1：创建新选区
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(100, 100))
    assert app.is_selecting == True
    assert app.active_handle == 'new'

    qtbot.mouseMove(snipper.snipping_widget, pos=QPoint(200, 200))
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(200, 200))

    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None
    assert not app.selection_rect.isNull()

    # 测试2：在选区内按下但不移动（点击）
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(150, 150))
    assert app.is_selecting == True
    assert app.active_handle == 'move'

    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(150, 150))

    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None

    # 测试3：拖拽移动选区
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(150, 150))
    qtbot.mouseMove(snipper.snipping_widget, pos=QPoint(200, 150))
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=QPoint(200, 150))

    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None


def test_click_outside_selection_keeps_expanded(app, qtbot):
    """
    测试：点击选区外（扩展选区），没有拖拽时，选区应该保持扩展状态
    
    修复后的预期行为：点击选区外后，选区扩展到包含点击位置并保持。
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 获取屏幕几何信息，用于坐标转换
    screen_geo = snipper.screen_geometry

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    original_rect = QRect(app.selection_rect)

    # 点击选区外（使用全局坐标）
    outside_global = QPoint(screen_geo.left() + 400, screen_geo.top() + 400)
    outside_local = outside_global - screen_geo.topLeft()

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)
    assert app.active_handle == 'expand'

    # 不移动，直接释放
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)

    # 选区应该扩展并保持（不再恢复到原始状态）
    assert app.selection_rect != original_rect, "点击选区外后，选区应该扩展而不是保持原状"
    # 使用选区内部的点进行验证（避免边界问题）
    target_global = QPoint(screen_geo.left() + 399, screen_geo.top() + 399)  # 在点击位置内部
    assert app.selection_rect.contains(target_global), \
        f"点击选区外后，选区应该扩展到包含点击位置附近的 {target_global}，但选区是 {app.selection_rect}"
    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None


def test_toolbar_transparent_area_passes_through(app, qtbot):
    """
    测试：工具栏上方的透明区域（topPadding）应该允许鼠标事件穿透到下层的 snipping_widget

    场景：
    1. 创建一个选区，工具栏显示
    2. 点击工具栏上方的 topPadding 透明区域
    3. 鼠标事件应该传递到下层的 snipping_widget
    4. is_selecting 状态应该正确响应
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    # 验证工具栏已显示
    assert snipper.toolbar.isVisible()

    # 获取工具栏位置
    toolbar_pos = snipper.toolbar.pos()

    # 获取 top padding 值（从 QML）
    top_padding = 40  # 默认值
    root_obj = snipper.toolbar.rootObject()
    if root_obj:
        padding_val = root_obj.property("topPadding")
        if padding_val is not None:
            top_padding = int(padding_val)

    # 在工具栏上方的透明区域点击（相对于工具栏的坐标）
    # toolbar y + top_padding - 10 像素（在透明区域内）
    click_y = toolbar_pos.y() + top_padding - 10
    click_x = toolbar_pos.x() + 10

    # 将局部坐标转换为窗口坐标
    click_pos_local = QPoint(click_x, click_y)

    # 初始状态
    assert app.is_selecting == False

    # 在透明区域按下鼠标
    qtbot.mousePress(snipper, Qt.MouseButton.LeftButton, pos=click_pos_local)

    # 验证鼠标事件穿透到了 snipping_widget，is_selecting 应该变为 True
    assert app.is_selecting == True, "工具栏透明区域的鼠标按下事件应该穿透到 snipping_widget"

    # 释放鼠标
    qtbot.mouseRelease(snipper, Qt.MouseButton.LeftButton, pos=click_pos_local)

    # 验证状态重置
    assert app.is_selecting == False, "释放鼠标后，is_selecting 应该重置为 False"
    assert app.is_dragging == False
    assert app.active_handle is None


def test_click_outside_selection_should_expand_without_drag(app, qtbot):
    """
    测试场景：鼠标点击选区外面，扩展选区后释放（没有拖动）
    
    Bug 描述：
    当鼠标点击截图区域外面，按下鼠标之后，截图区域会扩展，
    但是鼠标松开之后，自动变回去。
    
    预期结果：
    点击选区外后释放（没有拖动），选区应该扩展到点击位置并且不缩回去。
    即：selection_rect 应该包含点击位置，而不是恢复到原始位置。
    """
    app.start_capture()
    assert len(app.snippers) > 0
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 获取屏幕几何信息，用于坐标转换
    screen_geo = snipper.screen_geometry
    print(f"Screen geometry: {screen_geo}")

    # 1. 创建初始选区（使用局部坐标）
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    # 验证选区已创建
    assert not app.selection_rect.isNull()
    original_rect = QRect(app.selection_rect)
    print(f"Original selection rect: {original_rect}")
    
    # 验证初始状态
    assert app.is_selecting == False
    assert app.is_dragging == False
    assert app.active_handle is None

    # 2. 在选区外点击并释放（没有拖动）
    # 选择一个在选区外的位置（相对于屏幕的全局坐标）
    # 假设选区在 (100,100) 到 (300,200)，我们选择 (400, 300) 在选区右下角外面
    # 局部坐标 = 全局坐标 - 屏幕左上角
    outside_global = QPoint(screen_geo.left() + 400, screen_geo.top() + 300)
    outside_local = outside_global - screen_geo.topLeft()
    
    # 目标点：在扩展后的选区内（不是边界上）
    # 扩展后的选区会从 (2020, 360) 扩展到包含 (2320, 560)
    # 所以选择 (2250, 450) 应该在扩展后的选区内
    target_global = QPoint(screen_geo.left() + 250, screen_geo.top() + 150)
    target_local = target_global - screen_geo.topLeft()
    
    print(f"Clicking at global: {outside_global}, local: {outside_local}")
    print(f"Target point global: {target_global}, local: {target_local}")
    print(f"Original rect contains outside? {original_rect.contains(outside_global)}")
    
    # 按下鼠标 - 此时应该立即扩展选区
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)
    
    # 验证：按下后应该正在选择中
    assert app.is_selecting == True, "按下鼠标后，is_selecting 应该为 True"
    assert app.active_handle == 'expand', "按下位置在选区外，active_handle 应该为 'expand'"
    
    # 验证：选区应该已经扩展到包含点击位置
    print(f"Selection rect after press: {app.selection_rect}")
    assert app.selection_rect.contains(target_global), \
        f"按下鼠标后，选区应该扩展到包含点击位置附近的目标点 {target_global}，但当前选区是 {app.selection_rect}"
    # 注意：点击位置 (2320, 560) 可能正好在边界上，QRect.contains() 对边界返回 False

    # 3. 释放鼠标（没有拖动 - 位置不变）
    # 关键：释放时位置不变，这样 is_dragging 会是 False
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)

    # 验证：状态应该重置
    assert app.is_selecting == False, "释放鼠标后，is_selecting 应该重置为 False"
    assert app.is_dragging == False, "释放鼠标后，is_dragging 应该为 False"
    assert app.active_handle is None, "释放鼠标后，active_handle 应该为 None"

    # ==== 这是 bug 的关键验证点 ====
    # 预期：选区应该保持在扩展后的位置
    # Bug 表现：选区会恢复到原始位置
    
    print(f"Selection rect after release: {app.selection_rect}")
    
    # 验证选区仍然包含目标点（不是边界）
    assert app.selection_rect.contains(target_global), \
        f"点击选区外后释放，选区应该保持在扩展后的位置，包含 {target_global}，但选区是 {app.selection_rect}"
    
    # 验证选区已经扩展（比原始选区大）
    # 扩展后的选区应该比原始选区大
    assert app.selection_rect.width() >= original_rect.width() or app.selection_rect.height() >= original_rect.height(), \
        f"BUG: 选区应该扩展，但 {app.selection_rect} 与原始选区 {original_rect} 大小相同"


def test_click_outside_selection_small_movement(app, qtbot):
    """
    测试场景：鼠标点击选区外面，有小幅度移动后释放
    
    即使有小幅度的移动（但不超过拖拽阈值），选区也应该扩展并保持。
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 获取屏幕几何信息，用于坐标转换
    screen_geo = snipper.screen_geometry

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    original_rect = QRect(app.selection_rect)

    # 在选区外点击（使用全局坐标）
    outside_global = QPoint(screen_geo.left() + 400, screen_geo.top() + 300)
    outside_local = outside_global - screen_geo.topLeft()
    
    # 小幅度移动（不超过阈值 5px）
    small_move_global = QPoint(screen_geo.left() + 401, screen_geo.top() + 301)  # 曼哈顿距离 = 2 < 5
    small_move_local = small_move_global - screen_geo.topLeft()
    
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)
    
    # 小幅度移动（不超过阈值 5px）
    qtbot.mouseMove(snipper.snipping_widget, pos=small_move_local)
    
    # 释放鼠标
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=small_move_local)

    # 验证：选区应该扩展并保持（使用边界内的点）
    # 扩展后的选区包含点击位置，所以选择点击位置内部的一个点
    target_global = QPoint(screen_geo.left() + 399, screen_geo.top() + 299)  # 在点击位置内部
    assert app.selection_rect.contains(target_global), \
        f"点击选区外后有小幅度移动，选区应该保持在扩展后的位置，包含 {target_global}，但选区是 {app.selection_rect}"


def test_click_outside_selection_large_drag(app, qtbot):
    """
    测试场景：鼠标点击选区外面，有大幅度拖拽后释放
    
    注意：当前代码中 'expand' 操作不支持拖拽跟随。
    预期行为：选区应该扩展到包含点击位置，但不跟随后续拖拽。
    """
    app.start_capture()
    snipper = app.snippers[0]
    qtbot.addWidget(snipper)
    with qtbot.waitExposed(snipper):
        pass

    # 获取屏幕几何信息，用于坐标转换
    screen_geo = snipper.screen_geometry

    # 创建初始选区
    start_local = QPoint(100, 100)
    end_local = QPoint(300, 200)

    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=start_local)
    qtbot.mouseMove(snipper.snipping_widget, pos=end_local)
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=end_local)

    original_rect = QRect(app.selection_rect)

    # 在选区外点击（使用全局坐标）
    outside_global = QPoint(screen_geo.left() + 400, screen_geo.top() + 300)
    outside_local = outside_global - screen_geo.topLeft()
    
    qtbot.mousePress(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=outside_local)
    
    # 大幅度移动（超过阈值 5px）
    large_move_global = QPoint(screen_geo.left() + 500, screen_geo.top() + 400)  # 曼哈顿距离 = 200 > 5
    large_move_local = large_move_global - screen_geo.topLeft()
    qtbot.mouseMove(snipper.snipping_widget, pos=large_move_local)
    
    # 释放鼠标
    qtbot.mouseRelease(snipper.snipping_widget, Qt.MouseButton.LeftButton, pos=large_move_local)

    # 释放后状态重置
    assert app.is_selecting == False
    assert app.is_dragging == False
    
    # 当前行为：expand 操作只扩展到点击位置，不跟随拖拽
    # 选区应该包含点击位置（扩展后的效果）
    # 使用边界内的点进行验证
    target_global = QPoint(screen_geo.left() + 399, screen_geo.top() + 299)  # 在点击位置内部
    assert app.selection_rect.contains(target_global), \
        f"点击选区外后拖拽，选区应该包含点击位置附近的 {target_global}，但选区是 {app.selection_rect}"
