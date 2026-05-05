# Penguin Downloader TUI

为 [penguin-downloader](https://github.com/DeeChael/penguin-downloader) 开发的 TUI 客户端。

## 使用方法
下载对应的可执行程序后，放在一个合适的位置（该程序会在运行的位置自动生成一系列文件及文件夹）。

### Windows
双击可执行程序后即可运行，会自动打开一个终端窗口。

### macOS
在安放文件的位置打开终端，运行：
```shell
chmod +x penguin-downloader-tui
```
然后再使用终端运行
```shell
./penguin-downloader-tui
```
此时会出现弹窗提示不可用，打开“设置”->“隐私与安全性”->滑动到最底下允许运行 penguin-downloader。  
然后再运行。

### Linux
在安放文件的位置打开终端，运行：
```shell
chmod +x penguin-downloader-tui
```
然后再使用终端运行
```shell
./penguin-downloader-tui
```

### OpenHarmony
你需要先对编译好的工具进行签名才能使用。  
由于二进制签名限制企业用户申请，所以你只能通过自签的方式来使用，[教程](https://ystyle.top/2025/12/27/matebookpro-run-cangjie-compiler/)。