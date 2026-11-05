# tinim-gui / tinim-winit v0.1

> 已实现的最小闭环：`WindowEvent` → `RawInput` → `Ui` → `PaintList` / 字形上传 → wgpu。
> 第一版优先确认 API 形状，不追求完整的控件体系。

## 0. 最终取舍

- **没有 Id，也没有持久控件状态。** 对足够动态的程序（例如把 UI 操作暴露给脚本），Id 冲突几乎无法避免，所以 v0.1 不做身份系统。
- **Button 只支持 press，不支持 click。** 不保存按下/松开配对状态，只根据本帧输入返回 `pressed` 和 `down`。
- **TextLayout 独立缓存。** 缓存和控件无关，按内容与布局输入寻址；不缓存 prepared `Mesh`。
- **帧 API 使用闭包。** `Context::run(..., |ui| ...)` 保证 `Ui` 不会活过一帧。
- **`tinim-winit` 只做输入翻译。** 窗口、事件循环、surface、renderer 都由应用组装，示例负责演示。

## 1. 范围

v0.1 做：

- `Context` / `Ui` / `Frame` 的帧循环。
- 纵向/横向栈、内容尺寸、间距、嵌套作用域和继承裁剪。
- `Label`：返回占用的 `Rect`，不可交互。
- `Button`：返回 `Response`，支持 hover / press / down 的视觉反馈。
- 独立的 `TextLayout` 缓存。
- `WindowEvent` → `RawInput` 适配器。
- 一个 headless 单元测试集和一个可运行的 window + wgpu 示例。

v0.1 不做：

- Id、控件状态存储、click、焦点、键盘控件、文本编辑、IME。
- 滚动容器、弹层、z-order layer、动画、无障碍。
- flex/grid、百分比、交叉轴对齐、主轴 fill/权重。
- prepared mesh 缓存、脏区重绘、多窗口。
- 完整的 window / event-loop / wgpu runner。

## 2. crate 划分

```text
tinim-paint   math/color/Painter/PaintList       （已有）
tinim-text    shaping/layout/glyph atlas        （已有）
tinim-wgpu    PaintList -> wgpu                 （已有）
tinim-gui     Context/Ui/layout/widget/text cache
tinim-winit   winit WindowEvent -> RawInput
```

- `tinim-gui` 只依赖 `tinim-paint` 和 `tinim-text`，不依赖 winit/wgpu。
- `tinim-winit` 依赖 `winit` 和 `tinim-gui`，并重新导出 `winit`；它不持有窗口或渲染器。
- 示例同时使用 `tinim-wgpu` 和 `tinim-winit`，把输入、UI、渲染串起来。

## 3. API

```rust
// tinim-gui

pub struct Context { /* TextSystem, Painter, TextCache, Style, RawInput */ }
pub struct Ui<'a> { /* regions, &mut Context */ }

pub struct Frame<'a> {
    pub paint: &'a PaintList,       // 已是物理像素
    pub textures: TexturesDelta,    // 本帧字形上传
}

pub struct RawInput {
    pub pointer: Option<Pos2>,      // 逻辑点
    pub primary_down: bool,
    pub primary_pressed: bool,      // 本帧按下边沿
    pub primary_released: bool,     // 本帧松开边沿
    pub scroll: Vec2,
    pub modifiers: Modifiers,
    pub text: String,
}

pub struct Response {
    pub rect: Rect,
    pub hovered: bool,
    pub pressed: bool,              // 本帧在控件上按下
    pub down: bool,                 // 本帧指针在控件上且主键按住
}

impl Context {
    pub fn new(texture_id: TextureId) -> Self;

    pub fn text(&self) -> &TextSystem;
    pub fn text_mut(&mut self) -> &mut TextSystem;
    pub fn style(&self) -> &Style;
    pub fn style_mut(&mut self) -> &mut Style;

    pub fn clear_text_cache(&mut self);
    pub fn size(&self) -> Vec2;
    pub fn pixels_per_point(&self) -> f32;
    pub fn input(&self) -> &RawInput;

    pub fn run(
        &mut self,
        viewport: Vec2,
        pixels_per_point: f32,
        input: RawInput,
        build: impl FnOnce(&mut Ui<'_>),
    ) -> Frame<'_>;
}

impl Ui<'_> {
    pub fn label(&mut self, text: &str) -> Rect;
    pub fn button(&mut self, text: &str) -> Response;

    pub fn vertical<R>(&mut self, f: impl FnOnce(&mut Ui<'_>) -> R) -> R;
    pub fn horizontal<R>(&mut self, f: impl FnOnce(&mut Ui<'_>) -> R) -> R;

    pub fn allocate_space(&mut self, size: Vec2) -> Rect;
    pub fn available_size(&self) -> Vec2;
}
```

使用方式：

```rust
let frame = context.run(viewport, pixels_per_point, input, |ui| {
    ui.vertical(|ui| {
        ui.label("Hello");
        ui.horizontal(|ui| {
            if ui.button("Press me").pressed {
                println!("pressed");
            }
        });
    });
});

renderer.update_textures(device, queue, &frame.textures);
renderer.render(device, queue, &mut pass, screen, frame.paint);
```

`Frame` 借用 `Context`，`paint` 只在下一次 `run` 前有效。

## 4. 布局

每个作用域是一个 `Region`：

- `rect`：父级分配给它的区域。
- `clip`：父 clip 与 `rect` 的交集。
- `cursor`：下一个子控件的位置。
- `used`：子控件矩形的并集。
- `direction` 和 `spacing`。

规则：

- 根区域是垂直栈，范围为整个逻辑 viewport。
- `allocate_space(size)` 按当前主轴推进 `cursor`，返回子控件矩形，并更新 `used`。
- `vertical` / `horizontal` 结束后，把闭包内实际使用的 `used` 交给父级，父级按自己的主轴推进。
- `available_size()` 返回当前栈剩余空间；`label` 用其中的宽度作为文本换行上限。
- 尺寸只有内容尺寸：按钮由文本、padding 和 `min_size` 决定，label 由文本决定。
- 裁剪逐层继承并收窄；`Painter::set_clip_rect` 是绝对值，所以每次绘制都显式设置期望的 clip。
- 布局坐标是逻辑点；`Painter` 和 `TextSystem::prepare` 负责转换为物理像素。

## 5. 控件与交互

### Label

- 用 `TextSystem::layout` 测量，`allocate_space` 占位。
- 用 `TextSystem::prepare` 生成 mesh，再交给 `Painter`。
- 不参与交互，不产生 Id 或状态。

### Button

- 尺寸：`text_size + 2 * padding`，受 `min_size` 约束。
- 背景：圆角矩形；颜色为 `normal` / `hovered` / `pressed`。
- 文字居中，绘制时临时把 clip 收窄到按钮矩形。
- `hovered`：指针位于按钮矩形内，并且当前 clip 也包含指针。
- `pressed`：`hovered && input.primary_pressed`。
- `down`：`hovered && input.primary_down`。
- 不保存跨帧状态，不做按下/松开配对，所以没有 `clicked` 或 `active`。
- 布局默认不重叠；若调用方让控件重叠，后声明者会覆盖绘制，但 v0.1 没有 z-order 命中测试。

交互直接消费本帧 `RawInput`，因此控件下一帧消失、指针移出窗口或窗口失焦时，不会留下卡住的状态。

## 6. 帧流程

`Context::run` 内：

1. 清洗 viewport 和 `pixels_per_point`，保存本帧 `RawInput`。
2. 若 DPI 变化则重建 `Painter`（`Painter` 在构造时固定 ppp），否则更新尺寸。
3. `Painter::clear`，`reset_clip_rect`。
4. 创建根 `Ui`，立即执行闭包。
5. `Painter::finish()`，`TextSystem::take_textures()`。
6. 返回 `Frame { paint, textures }`。

文本缓存跨帧保留；`PaintList` 和字形上传只代表当前帧。

## 7. 文本缓存

缓存与控件完全分离，按内容寻址：

- key：`text` + `LayoutKey`，其中 `LayoutKey` 由规范化后的布局样式和换行宽度组成。
- `LayoutStyle` 只包含影响布局的字段：`font_size`、`line_height`、`letter_spacing`、`direction`。
- `color` 不参与 layout key；同一份布局可以用不同颜色调用 `prepare` 绘制。
- `f32` 用 `to_bits()` 比较，但先做归一化：非法值按 layout 的规则处理，`-0.0` 归一为 `0.0`，`None` 与无穷大宽度等价。
- value：`TextLayout`（直接内联存储），默认容量 256，按最近使用时间淘汰。`TextCache::layout` 返回借用；调用方在 layout 借用存活期间只访问与缓存无关的字段。
- 相同文本在不同位置可以共享同一个 layout。
- `Context::clear_text_cache()` 可在添加字体等改变 shaping 结果的操作后手动调用。

不缓存 prepared `Mesh`：

- atlas 增长会重新打包 UV，旧 mesh 的 UV 会失效。
- mesh 使用绝对物理坐标，布局移动后需要重建。
- `TextLayout` 已经省掉最贵的 shaping，`prepare` 每帧只生成四边形。

## 8. tinim-winit

```rust
pub struct WinitInput { /* 累积一帧内的事件 */ }

impl WinitInput {
    pub fn new(scale_factor: f32) -> Self;
    pub fn scale_factor(&self) -> f32;
    pub fn on_window_event(&mut self, event: &WindowEvent);
    pub fn take(&mut self) -> RawInput;
}
```

事件映射：

- `CursorMoved`：物理坐标 ÷ `scale_factor` → 逻辑点。
- `CursorLeft`：清空指针位置。
- `MouseInput`：只处理左键；`Pressed` / `Released` 产生本帧边沿，并维护 `primary_down`。
- `MouseWheel`：`LineDelta` 按每行 50 点换算，`PixelDelta` 除以 `scale_factor`。
- `ModifiersChanged`：映射 shift / ctrl / alt / super。
- `KeyboardInput`：非 repeat 的按下事件追加 `event.text`。
- `Focused(false)`：清空按住状态和边沿，避免切窗口后卡住。
- `ScaleFactorChanged`：更新内部缩放，并清空旧逻辑坐标的指针位置。
- `take()`：返回快照；清空边沿、scroll 和 text，保留指针、按住状态和修饰键。

`Resized` 不进入 `RawInput`；应用自己用它更新 surface 和 viewport。

## 9. 验证

已覆盖的 headless 测试：

- 纵向/横向栈的位置、尺寸和间距。
- label 返回分配的矩形。
- 指针在按钮内/外时的 press 行为。
- 文本缓存命中、不同宽度区分、容量淘汰。

```text
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

图形示例：

```text
cargo run -p tinim-winit --example button
```

示例需要显示环境；当前环境只做编译检查。

## 10. 已知限制

- 没有 click、焦点、键盘控件和文本编辑。
- 没有滚动、弹层和 z-order 命中测试。
- 没有交叉轴对齐、fill 和主轴权重。
- 没有 Id/状态系统，因此控件不能在帧之间保存选择、展开等状态；需要状态的控件应等到设计出适合动态脚本场景的寻址方式后再加。
