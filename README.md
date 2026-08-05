# apple-matting-cli

[English](./README.en.md)

基于 Apple Vision 和 Core Image 的 macOS 本地抠图工具，提供单图命令、有限并发的
批量命令，以及供本地程序调用的 HTTP 服务。

## 系统要求

- macOS 14.0 或更高版本
- Apple Silicon 或 Intel Mac
- 不需要云端 API，也不需要下载模型

实际抠图依赖 `VNGenerateForegroundInstanceMaskRequest`，因此只支持 macOS。

## 安装

### Homebrew

如果尚未安装 Homebrew，请先访问官方网站：

<https://brew.sh/>

使用以下命令安装：

```bash
brew install choco9527/tap/apple-matting-cli
```

该命令会自动添加 `choco9527/tap`；它由本项目维护，不属于 Homebrew Core 官方 Formula。

### GitHub Release

在 [GitHub Releases](https://github.com/choco9527/apple-matting-cli/releases)
下载与 Mac 架构对应的压缩包：

- Apple Silicon 使用 `macos-arm64`
- Intel Mac 使用 `macos-x86_64`

使用 `SHA256SUMS` 校验压缩包，解压后将 `apple-matting-cli` 放入 `PATH` 目录。

## 从源码构建

安装 Rust 工具链和 Xcode Command Line Tools 后执行：

```bash
cargo test --locked
cargo build --release --locked --bin apple-matting-cli
./target/release/apple-matting-cli --help
```

最终二进制位于 `target/release/apple-matting-cli`。

## 单图处理

```bash
apple-matting-cli input.jpg
apple-matting-cli input.jpg output.png
apple-matting-cli input.jpg -o output.png
apple-matting-cli input.jpg --output output.png
apple-matting-cli input.jpg --crop -o output.png
apple-matting-cli input.jpg --background white -o output.png
apple-matting-cli input.jpg --background "#FFCC00" -o output.png
```

没有指定输出路径时，会在原图旁生成 `input_nobg.png`。背景默认透明；
`--background` 支持 `transparent`、`white`、`black` 或带引号的 `#RRGGBB`
颜色。所有结果均为 PNG；`--crop` 会把结果裁剪到检测到的前景边界。

## 批量处理

```bash
apple-matting-cli --batch ./input -o ./output
apple-matting-cli --batch ./input -o ./output --recursive
apple-matting-cli --batch ./input -o ./output --crop --recursive --jobs 3
apple-matting-cli --batch ./input -o ./output --background white --jobs 3
```

批量行为：

- 支持 JPG、JPEG、PNG、WEBP、BMP。
- 强制使用独立输出目录，不会写入输入目录树。
- 默认只处理目录第一层；添加 `--recursive` 后递归处理。
- 递归模式下保留相对目录结构。
- 默认三个工作线程，`--jobs` 允许 1 到 64。
- 整批任务只需设置一次 `--background`，默认透明。
- 单张失败不会中断整批任务。
- 成功输出路径写入 stdout，错误和最终汇总写入 stderr。
- 全部成功返回退出码 `0`；任意图片失败返回 `1`。
- 多张输入映射到同一个 PNG 结果时，会在开始前拒绝处理。
- 输入图片超过 3200 万像素时会输出内存压力警告。系统响应变慢时可以降低
  `--jobs`，但图片尺寸才是内存占用的主要因素。

## 本地 HTTP 服务

启动服务：

```bash
apple-matting-cli --server --port 8080
```

通过 multipart 字段 `file` 上传单张图片：

```bash
curl -X POST -F "file=@input.jpg" \
  http://127.0.0.1:8080/matting --output output.png
```

添加 `-F "crop=true"` 可裁剪主体，添加 `-F "background=#FFFFFF"` 可设置纯色背景。
成功响应为 `image/png`；背景颜色无效时返回 HTTP `400`，抠图失败返回 HTTP `422`。

服务监听 `0.0.0.0` 并开放 CORS，目前没有内置鉴权、上传大小限制、限流、队列或
全局并发控制。只应在可信网络使用，公网部署必须放在带鉴权和限流的代理后面。

## 退出码

| 退出码 | 含义 |
| ---: | --- |
| `0` | 成功 |
| `1` | 抠图、批量、文件或服务错误 |
| `2` | 命令参数错误 |

## 批量性能基准

正式发布前可以执行可重复的本地基准：

```bash
scripts/benchmark-batch.sh ./sample.png
```

默认测试 100 张 4000×4000 图片和三个工作线程。可以通过 `BENCHMARK_COUNT`、
`BENCHMARK_SIZE`、`BENCHMARK_JOBS` 调整规模。脚本会使用临时目录，输出 macOS
耗时和内存指标，校验结果数量，并在退出时自动删除生成文件。

## 完整命令形式

```text
Usage:
  apple-matting-cli <input-image> [-o|--output <output-png>] [--crop] [--background <color>]
  apple-matting-cli --batch <input-dir> -o <output-dir> [--crop] [--background <color>] [--recursive] [--jobs <count>]
  apple-matting-cli --server [--port <port>]
  apple-matting-cli --version
```

## 许可证

使用 [GPL-3.0-only](./LICENSE) 发布。
