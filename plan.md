# tinim-gui 计划（v0.1 = tinim-paint + tinim-wgpu）

> tiny immediate mode GUI。绘制模型**照搬 egui**（`Shape` → tessellate → `ClippedPrimitive`），不做自创格式；
> 去掉 egui 里跟窗口/多 viewport/回调相关的一切。v0.1 只做绘制层，`tinim-gui` 与 `tinim-winit` 留到 v0.2。

## 修订记录

- **r4**（2026-10-31）：范围收窄到 `tinim-paint` + `tinim-wgpu`；绘制改为照搬 egui 的 `Vec<ClippedPrimitive>`（无 `PaintCallback`）；text 不做 feature；ID 方案定为 egui 式 `Id`（v0.2 实现）；补环境与 git 约定。
- r3：clip 改 scissor 分批、glam、去 cursor icon / 按需重绘 / rust-toolchain.toml。
- r2：初稿。

---

## 0. 范围

| 版本 | 内容 |
|---|---|
| **v0.1（现在）** | `tinim-paint`（数学、`Shape`、tessellation、文字与 atlas）+ `tinim-wgpu`（渲染 `ClippedPrimitive`）。交付物：一条命令渲染出带文字的"按钮底板" PNG |
| v0.2 | `tinim-gui`（`Context`/`Ui`/布局/交互/`button`）+ `tinim-winit` → 真正可点的按钮 |
| 以后 | focus/文本编辑、`ScrollArea`、`Checkbox`/`Slider`、Popup layer、SDF 文字 |

不做（整个项目）：多窗口/viewport、`PaintCallback`、光标图标、按需重绘调度、原生菜单、剪贴板、IME、无障碍、wasm、非 wgpu 后端、serde 持久化。

---

## 1. crate 划分

```
tinim-paint ← tinim-gui ← tinim-winit
     ↑
tinim-wgpu
```

| crate | 状态 | 依赖 | 职责 |
|---|---|---|---|
| `tinim-paint` | **v0.1** | `glam`、`cosmic-text`、`bytemuck` | 数学、颜色、`Shape`、`Mesh`、tessellation、`Fonts`（cosmic-text + glyph atlas） |
| `tinim-wgpu` | **v0.1** | `tinim-paint`、`wgpu` | pipeline、顶点/索引缓冲、texture atlas 上传、scissor 批绘制 |
| `tinim-gui` | v0.2 | `tinim-paint` | `Context`/`Ui`/`Style`/交互 |
| `tinim-winit` | v0.2 | `tinim-gui`、`winit` | 事件翻译（不创建 surface） |

硬规则：

1. `tinim-paint` 不认识 wgpu/winit/窗口。
2. `tinim-wgpu` 不认识 cosmic-text（只吃 `ClippedPrimitive` + `TexturesDelta`）。
3. 绘制格式只有一个：egui 那套。**不发明第二套。**
4. 版本统一（`0.1.0-alpha.1`），四个 crate 同步发版。

---

## 2. 依赖版本（2026-10 实际查询）

| 依赖 | 版本 | 备注 |
|---|---|---|
| `wgpu` | `30.0.1` | 只在 `tinim-wgpu` |
| `glam` | `0.34.1` | `Pos2 = Vec2` |
| `cosmic-text` | `0.19.0` | 默认 features（含 `swash`、`fontconfig`），MSRV 1.89 |
| `bytemuck` | `1.x` | `Vertex: Pod` |
| `png` | dev-dep | 只给 headless 示例输出 PNG |
| `pollster` | dev-dep | block_on |

workspace `rust-version = "1.89"`，不加 `rust-toolchain.toml`。

---

## 3. 绘制模型（照搬 egui，去掉 callback）

```rust
// 输入：每帧构建的形状列表（带 clip）
pub struct ClippedShape { pub clip_rect: Rect, pub shape: Shape }

pub enum Shape {
    Rect(RectShape),          // rect, corner_radius, fill, stroke
    Circle(CircleShape),      // center, radius, fill, stroke
    Line(LineShape),          // points, width, color
    Text(TextShape),          // galley: Arc<Galley>, pos, color
    Mesh(Mesh),               // 逃生口：自定义三角形（自带 texture_id）
}

// 输出：一个 primitive = 一个 mesh + 一个 scissor
pub struct ClippedPrimitive { pub clip_rect: Rect, pub mesh: Mesh }

pub struct Mesh {
    pub indices: Vec<u32>,
    pub vertices: Vec<Vertex>,
    pub texture_id: TextureId,   // v0.1 只有 0 = 字体 atlas（含白像素）
}

#[repr(C)]
pub struct Vertex {              // 16 bytes，和 egui 一致
    pub pos: [f32; 2],           // 物理像素
    pub uv: [u16; 2],            // normalized（0..=65535）
    pub color: [u8; 4],          // sRGB，straight alpha
}
```

与 egui 的**唯一**差异：`Primitive` 只剩 `Mesh`，所以不需要单变体枚举，直接 `ClippedPrimitive { clip_rect, mesh }`。没有 `PaintCallback`（按你的要求），也就没有"半途插入自定义绘制"的排序问题。

### 3.1 三角形化

```rust
pub struct Tessellator { /* 复用缓冲，避免每帧分配 */ }
impl Tessellator {
    pub fn tessellate(&mut self, shapes: &[ClippedShape], ppp: f32) -> Vec<ClippedPrimitive>;
}
```

- 一个 `ClippedShape` → 一个 `ClippedPrimitive`（不做跨形状合并，简单优先）。
- `Rect`：圆角路径用每角 8 段近似；填充用扇形（凸多边形，安全）；描边用"外轮廓 + 内缩 width 的轮廓"之间的四边形带。
- `Circle`：填充扇形；描边两层圆环。
- `Line`：每段一个四边形（miter 接头不做，调试线足够）。
- **形状不做抗锯齿**；文字的抗锯齿来自 glyph coverage。
- 逻辑点 → 物理像素：`pos * ppp`，轴对齐矩形边界先 `round_to_pixels`，避免 1px 缝。
- 内部所有几何用 `f32`，`Mesh.index` 用 `u32`（文字很容易超 65535 顶点）。

### 3.2 渲染（`tinim-wgpu`）

```rust
pub struct ScreenDescriptor { pub size_in_pixels: [u32; 2] }

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, target_format: wgpu::TextureFormat) -> Self;
    pub fn update_textures(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, delta: &TexturesDelta);
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        screen: ScreenDescriptor,
        prims: &[ClippedPrimitive],
    );
}
```

- 一条 pipeline、一个 bind group layout（uniform: 屏幕尺寸 + 一个 texture/sampler）。`target_format` 由调用方给（surface 的 format）。
- 顶点布局：`Float32x2` / `Unorm16x2` / `Unorm8x4`（uv 归一化到 0..1，省 4 字节）。
- 每个 primitive：`set_scissor_rect(clip)` → `draw_indexed`。和 egui 一样，clip 不进顶点、不用 `discard`。
- scissor 三个坑（必须写进代码注释并有单测）：clamp 到 attachment 尺寸（越界是 validation error）、宽高为 0 跳过、浮点边界向外取整（`floor/min`、`ceil/max`）。
- 顶点/索引缓冲按需增长（2 的幂），`queue.write_buffer` 上传。
- 颜色：顶点色是 sRGB，shader 里转线性 + 预乘，混合 `One / OneMinusSrcAlpha`。
- texture：`Rgba8Unorm`（非 sRGB）。v0.1 只用 alpha/覆盖率，shader 里 `a = color.a * tex.a`、`rgb = linear(color.rgb) * tex.rgb * a`。白像素放在 (0,0)，所有纯色几何的 uv = (0,0)。

### 3.3 纹理增量（照搬 egui 的形状，收窄）

```rust
pub struct ImageDelta { pub pos: Option<[u32; 2]>, pub size: [u32; 2], pub rgba: Vec<u8> }
pub struct TexturesDelta { pub set: Vec<(TextureId, ImageDelta)>, pub free: Vec<TextureId> }
```

v0.1 只需要 `set`（atlas 扩容/初次上传），`free` 保留字段但不用。

---

## 4. 文字（核心依赖，不做 feature）

`tinim-paint::text`：

- `Fonts` = cosmic-text `FontSystem` + `SwashCache` + `TextAtlas`。
- atlas：`Rgba8Unorm` 2048² 起步，shelf packing，1px padding；满了先扩到 4096²，再满清空重建（`warn!`）。白像素常驻 (0,0)。
- 栅格化键：`(font_id, glyph_id, px_size 量化到 0.25, subpixel_x 4 桶, subpixel_y 4 桶)`。
- `layout(text, font, wrap) -> Arc<Galley>`：`Galley` 只含行 + 字形位置（逻辑点），`tinim-wgpu` 完全不认识 cosmic-text。
- v0.1 只暴露默认 proportional + monospace 两个族，不引 `FontDefinitions`。
- **不做**：编辑、IME、选区、emoji 彩色字体。

---

## 5. `tinim-gui` 的两个既定决策（v0.2 才实现，先写死）

**布局**：只有"顺序分配矩形"。`ui.allocate_space(size)` 是唯一原语，控件尺寸自己算完，不依赖父容器；没有 flex/grid/自动换行流式布局/两趟测量。唯一例外是 `label` 按 `max_rect.width()` 换行（cosmic-text 白送）。

**ID**：采用 **egui 式 `Id(u64)`**，不是位置索引。

```rust
Id::new(source)                     // fnv1a hash，5 行，确定性
parent_id ^ hash(source)            // 父子链
ui.next_auto_id()                   // 帧内计数器，每帧重置
ui.push_id(key, |ui| ...)           // 需要时的手动作用域
```

理由：位置索引在"条件控件 / 重排 / 列表"下会**静默**把按下状态、焦点、滚动位置绑到别的控件上，属于最难查的 bug；而 `Id` 的实现成本约 40 行，且 `Response` 带 `id` 后调用方还能自己做状态。v0.1 没有 gui，所以这条先放着，但不要再改。

---

## 6. 环境与验证

- 容器无 GPU、无 `/dev/dri`、无 `DISPLAY`。已 `dnf install mesa-vulkan-drivers vulkan-tools`，`vulkaninfo` 可见 **llvmpipe / lavapipe**（CPU Vulkan 1.4）。
- 因此：
  - **headless 渲染**（离屏 texture → `COPY_SRC` 回读 → PNG）是本环境唯一能跑通的方式，也是主要验证手段；
  - 依赖 surface 的示例（窗口）只能保证编译，跑不了。
- `tinim-wgpu/examples/headless.rs`：
  1. 离屏渲染一张 480×240 的图；
  2. 画一个圆角矩形 + 描边 + 一条线（M2 后加文字"Click me"）；
  3. 回读像素：断言背景色、按钮中心色、边框色；
  4. 写 `target/headless.png` 供人眼检查。
- 若 wgpu 找不到 adapter：优先检查 `VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json`，并只开 `Backends::VULKAN` + `force_fallback_adapter`。

---

## 7. 里程碑

进度：M0、M1 已完成（2026-10-31），当前卡在 M2（cosmic-text 文字 + atlas）。

| # | 内容 | 验收 |
|---|---|---|
| **M0** ✅ | workspace、`tinim-paint` 数学/`Color32`/`Shape`/`Mesh`/`ClippedPrimitive` | `cargo test -p tinim-paint` 通过；类型即文档 |
| **M1** ✅ | Tessellator（Rect/Circle/Line）+ `tinim-wgpu`（pipeline/buffer/scissor）+ headless 示例 | 5 个 primitive、7 个像素采样点断言通过（lavapipe）；`tests/render.rs` 在无 adapter 时自动跳过 |
| **M2** | `Fonts`（cosmic-text 布局 + atlas）+ `Shape::Text` + 白像素复用 | PNG 里出现清晰文字；atlas 增量上传正确；无渗色 |
| **M3** | 圆角/描边细节、索引缓冲增长、文档、README（known limitations） | `cargo clippy -D warnings`、`cargo doc` 干净；PNG 里按钮观感可接受 |
| v0.2 | `tinim-gui` + `tinim-winit` + 交互 | 可点按钮 + 计数 label |

---

## 8. 测试

- 单测（无 GPU）：`Rect` 运算、tessellation 不变量（索引不出界、索引恰好覆盖所有顶点、三角形方向一致、uv 在 0..1）、圆角路径点数、`round_to_pixels`、scissor 取整辅助函数。
- GPU 测试：`#[test]` 里跑 headless 一帧 → 回读 → 断言 3–5 个采样点。不做逐像素 golden（太脆）。
- 调试：`TINIM_DEBUG_PAINT=1` 时把 clip rect 用 `Shape::Line` 画出来。

---

## 9. 风险

| 风险 | 处理 |
|---|---|
| wgpu 30 API 与旧笔记不一致（`DeviceDescriptor`、`PollType`、`TexelCopy*`） | 以编译器和 vendor 源码为准，别凭记忆写 |
| cosmic-text 0.19 的 `SwashCache`/`LayoutGlyph` 用法变了 | M2 先写 20 行 spike 跑通 `get_image` + `physical()` |
| scissor 越界/空批次 | clamp + skip + 单测 |
| 颜色空间（发灰/发白） | 统一约定：顶点 sRGB → 线性混合；写一个 2×2 像素的纯色断言 |
| lavapipe 慢 | 只跑 480×240、一帧；不要做动画 loop 测试 |
| 范围蔓延 | 想加布局/主题/动画 → 先改 plan.md |

---

## 10. git 约定

- 仓库在 `/workspace/tinim-gui`（已 `git init`），`plan.md` 入库。
- `master` 直接提交（个人项目），但每次提交保持"可编译 + 测试绿"。
- commit message：`type(scope): summary`，如 `feat(paint): tessellate rounded rects`、`fix(wgpu): clamp scissor rect`。
- `target/` 已在 `.gitignore`；`Cargo.lock` **入库**（库 + 示例的 workspace，锁版本更利于复现）。
- 里程碑完成打 tag：`v0.1.0-alpha.1`。

---

## 11. 目录结构

```
tinim-gui/
├─ Cargo.toml                 # [workspace]
├─ plan.md
├─ README.md
├─ crates/
│  ├─ tinim-paint/
│  │  ├─ Cargo.toml
│  │  └─ src/{lib,math,color,shape,mesh,textures,tessellate,text}.rs
│  └─ tinim-wgpu/
│     ├─ Cargo.toml
│     ├─ src/{lib,renderer,texture,shader.wgsl}
│     └─ examples/headless.rs
└─ (v0.2) crates/{tinim-gui,tinim-winit}, examples/hello_button
```
