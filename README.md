# aweme-db-decrypt

[![release](https://github.com/chinleez/aweme_db_decrypt/actions/workflows/release.yml/badge.svg)](https://github.com/chinleez/aweme_db_decrypt/actions/workflows/release.yml)

抖音极速版 (`com.ss.android.ugc.aweme.lite`) IM 加密数据库解密工具:把 WCDB 2 / SQLCipher v3 加密的 IM 库还原成标准 SQLite,也可直连加密库查询、交互、实时监听。源文件不会被修改。

## 适用文件

| 文件名 | 模块 | Schema |
|---|---|---|
| `encrypted_<uid>_im.db` | IM Core(消息主库) | v73 |
| `encrypted_sub_<uid>_im.db` | IM Core 子进程库 | v73 |
| `encrypted_im_biz_<uid>.db` | IM Biz(联系人/业务库) | v56 |

`<uid>` 为登录用户的纯数字 uid;文件名被改过时用 `-u <uid>` 手动指定。

## 安装

从 GitHub Releases 下载对应平台二进制:

| 平台 | 文件 |
|---|---|
| macOS | `aweme-db-decrypt-macos-arm64` / `-x86_64` |
| Linux | `aweme-db-decrypt-linux-x86_64` / `-arm64` |
| Windows | `aweme-db-decrypt-windows-x86_64.exe` / `-arm64.exe` |
| Android | `aweme-db-decrypt-android-arm64` / `-armv7` / `-x86_64` |

```bash
chmod +x aweme-db-decrypt-*
# macOS 被 Gatekeeper 拦截时:
xattr -d com.apple.quarantine aweme-db-decrypt-macos-arm64
```

也可以自己编译:`cargo build --release`;多平台产物用 `./build-all.sh`,未安装的 target/NDK 会自动跳过。

## 用法

| 子命令 | 用途 |
|---|---|
| `decrypt` | 解密为 plaintext SQLite 落盘,默认输出 `plain_*.db` |
| `query` | 直连加密库跑一次性 SQL,输出表格 / `--json` / `--csv` |
| `shell` | 交互式 SQLite REPL |
| `watch` | 实时轮询新消息,可用 `--biz-db` / `--fts-db` 补昵称与可读文本 |

不带子命令时按 `decrypt` 解释,旧用法不变。`query` / `shell` 默认只读;`--write` 的改动只作用于内部临时副本,进程退出即丢弃。

```bash
# 解密
aweme-db-decrypt encrypted_<UID>_im.db
aweme-db-decrypt -u <UID> dump.bin -o im.db --force   # 文件改名时

# 查询
aweme-db-decrypt query encrypted_<UID>_im.db -e "SELECT count(*) FROM msg"
aweme-db-decrypt query encrypted_<UID>_im.db --json -e "SELECT * FROM msg LIMIT 5"
aweme-db-decrypt query encrypted_<UID>_im.db -f probes.sql

# REPL
aweme-db-decrypt shell encrypted_<UID>_im.db

# 实时监听:默认从当前最新消息开始,只打印之后的新消息
aweme-db-decrypt watch encrypted_<UID>_im.db --recent 20 \
    --biz-db encrypted_im_biz_<UID>.db
aweme-db-decrypt watch encrypted_<UID>_im.db --output jsonl   # 机器处理
```

完整选项见 `aweme-db-decrypt --help` 或 `aweme-db-decrypt <子命令> --help`。

## 取 DB 与在 Android 上跑

DB 在应用沙盒 `/data/data/com.ss.android.ugc.aweme.lite/databases/`,需 root 或 debug 包:

```bash
adb shell "su -c 'cp /data/data/com.ss.android.ugc.aweme.lite/databases/encrypted_*.db* /sdcard/'"
adb pull /sdcard/encrypted_<UID>_im.db .
adb pull /sdcard/encrypted_<UID>_im.db-wal .   # 有 WAL 就一起拉,工具会自动合并
```

不想 pull 回桌面时,把 Android 二进制直接推到手机上执行(Termux 或 root shell 均可):

```bash
adb push aweme-db-decrypt-android-arm64 /data/local/tmp/
adb shell
$ chmod +x /data/local/tmp/aweme-db-decrypt-android-arm64
$ su -c '/data/local/tmp/aweme-db-decrypt-android-arm64 \
    watch /data/data/com.ss.android.ugc.aweme.lite/databases/encrypted_<UID>_im.db \
    --biz-db /data/data/com.ss.android.ugc.aweme.lite/databases/encrypted_im_biz_<UID>.db \
    --fts-db /data/data/com.ss.android.ugc.aweme.lite/databases/encrypted_<UID>_im_fts_split.db \
    --post-url http://<LAN_IP>:8787/api/messages'
```

`watch` 只读打开原始库,不复制临时 DB;`--post-url` 只支持 `http://`,默认只上报新增消息,需要连 `--recent` 历史一起上报时加 `--post-recent`。ABI 用 `adb shell getprop ro.product.cpu.abi` 确认(现代设备一般是 `arm64-v8a`)。

## 故障排查

| 现象 | 原因 / 解法 |
|---|---|
| `cannot infer DB kind / uid from filename` | 文件被改过名 → 加 `-u <uid>` |
| `decryption failed (wrong password or wrong cipher params)` | uid 不对;不是这两类 IM 库;或 APK 升级换算法 |
| `output ... already exists` | 加 `-f` / `--force` |
| `Error 14: SQLITE_CANTOPEN` | 父目录无写权限或路径含特殊字符 |
| 拉文件 `Permission denied` | DB 在沙盒里,需 root 或 debuggable 包 |
