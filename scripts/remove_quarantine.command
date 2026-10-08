#!/bin/zsh

set -u

APP_NAME="Agent Session Manager.app"
QUARANTINE_ATTR="com.apple.quarantine"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

find_default_target() {
  local candidates=(
    "/Applications/$APP_NAME"
    "$SCRIPT_DIR/$APP_NAME"
    "$SCRIPT_DIR/../$APP_NAME"
    "$HOME/Applications/$APP_NAME"
  )

  local candidate
  for candidate in "${candidates[@]}"; do
    if [[ -e "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  return 1
}

TARGET_PATH="${1:-}"

if [[ -z "$TARGET_PATH" ]]; then
  TARGET_PATH="$(find_default_target || true)"
fi

if [[ -z "$TARGET_PATH" ]]; then
  echo "未自动找到应用: $APP_NAME"
  echo "你可以把 $APP_NAME 拖到这个终端窗口上，或手动输入完整路径。"
  echo
  read "TARGET_PATH?请输入应用路径: "
fi

# 去除用户拖拽路径可能自带的单双引号或末尾空格
TARGET_PATH="${TARGET_PATH#\'}"
TARGET_PATH="${TARGET_PATH%\'}"
TARGET_PATH="${TARGET_PATH#\"}"
TARGET_PATH="${TARGET_PATH%\"}"
TARGET_PATH="${TARGET_PATH## }"
TARGET_PATH="${TARGET_PATH%% }"

if [[ -z "$TARGET_PATH" ]]; then
  echo "未提供路径，脚本结束。"
  exit 1
fi

if [[ ! -e "$TARGET_PATH" ]]; then
  echo "目标不存在: $TARGET_PATH"
  exit 1
fi

echo "正在解除 macOS Gatekeeper 隔离属性 (com.apple.quarantine):"
echo "$TARGET_PATH"
echo

if xattr -dr "$QUARANTINE_ATTR" "$TARGET_PATH" 2>/dev/null; then
  echo "✓ 处理完成！已解除隔离属性。"
  echo "现在可以正常双击打开应用（若提示未知名开发者，可在系统设置->隐私与安全性中点击'仍要打开'）。"
else
  # 如果没有隔离属性，xattr 可能会返回非零，检查属性是否已不存在
  if ! xattr -p "$QUARANTINE_ATTR" "$TARGET_PATH" 2>/dev/null; then
    echo "✓ 目标应用当前未受隔离属性限制，可直接打开。"
  else
    echo "✗ 处理失败，请检查文件权限（必要时可用 sudo 执行）。"
    exit 1
  fi
fi

echo
read "?按回车键退出..."
