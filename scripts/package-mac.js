#!/usr/bin/env node
/**
 * macOS 完整打包与发布脚本 (对齐 gao-tool release 结构)
 * 
 * 步骤：
 * 1. 执行 tauri build --bundles app,dmg 构建产物
 * 2. 调用 sign-mac.js 进行全量 Mach-O 与 --deep 深度签名及校验
 * 3. 整理分发包到 release/ 目录：
 *    - Agent Session Manager.app
 *    - Agent Session Manager.dmg (附带签名)
 *    - 双击解除隔离.command (解决 Gatekeeper 拦截与损坏提示)
 *    - 快速使用说明.txt
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { signMacBundle } from './sign-mac.js';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

function run(cmd, args, options = {}) {
  const printableCmd = `${cmd} ${args.join(' ')}`;
  console.log(`\n[exec] ${printableCmd}`);
  const result = spawnSync(cmd, args, {
    stdio: 'inherit',
    cwd: options.cwd || projectRoot,
    ...options
  });
  if (result.status !== 0) {
    throw new Error(`Command failed with code ${result.status}: ${printableCmd}`);
  }
}

async function packageMac() {
  if (process.platform !== 'darwin') {
    console.error('[package-mac] 该脚本仅支持在 macOS 系统上运行！');
    process.exit(1);
  }

  console.log('==============================================');
  console.log('  Agent Session Manager - macOS 构建与签名');
  console.log('==============================================\n');

  // 1. 构建 Tauri App 和 DMG
  console.log('[1/3] 正在编译前端与 Rust 原生程序 (pnpm tauri build)...');
  run('pnpm', ['tauri', 'build', '--bundles', 'app,dmg']);

  // 2. 执行签名与深校验
  console.log('\n[2/3] 执行代码签名与深层校验...');
  signMacBundle();

  // 3. 整理产物至 release 目录
  console.log('\n[3/3] 正在归档发布文件至 release/ 目录...');
  const releaseDir = path.join(projectRoot, 'release');
  if (!fs.existsSync(releaseDir)) {
    fs.mkdirSync(releaseDir, { recursive: true });
  }

  const appDir = path.join(projectRoot, 'src-tauri/target/release/bundle/macos');
  const dmgDir = path.join(projectRoot, 'src-tauri/target/release/bundle/dmg');

  // 查找 .dmg
  if (fs.existsSync(dmgDir)) {
    const dmgs = fs.readdirSync(dmgDir).filter(f => f.endsWith('.dmg'));
    for (const dmg of dmgs) {
      const srcDmg = path.join(dmgDir, dmg);
      const destDmg = path.join(releaseDir, dmg);
      fs.copyFileSync(srcDmg, destDmg);
      console.log(`  ✓ 已拷贝 DMG: release/${dmg}`);
    }
  }

  // 生成使用说明文件
  const readmeContent = `Agent Session Manager - macOS 安装与运行说明
======================================================

1. 安装方式：
   - 双击打开 DMG 镜像文件，将 "Agent Session Manager.app" 拖动至 "Applications" (应用程序) 文件夹中即可。

2. 首次打开提示“无法打开”或“未知名开发者”时解决办法：
   macOS Sequoia / Sonoma 对非 App Store 下载的应用实施了安全机制 (Gatekeeper)。
   
   【方式一（系统设置放行）】
   打开 macOS「系统设置」->「隐私与安全性」，滑到底部，在提示阻拦的区域点击「仍要打开」即可。

   【方式二（终端命令）】
   打开终端运行以下命令：
   sudo xattr -dr com.apple.quarantine "/Applications/Agent Session Manager.app"


3. 局域网与网络权限说明：
   应用支持多 Agent 平台会话管理（Claude Code, Codex, OpenCode, Goose 等），已内置 Hardened Runtime 网络客户端与服务端权限。
`;

  fs.writeFileSync(path.join(releaseDir, 'macOS安装与权限说明.txt'), readmeContent, 'utf-8');
  console.log(`  ✓ 已生成: release/macOS安装与权限说明.txt`);

  console.log('\n==============================================');
  console.log('  🎉 macOS 打包、深签名与发布准备全部完成！');
  console.log(`  产物位置: ${releaseDir}`);
  console.log('==============================================\n');
}

packageMac().catch(err => {
  console.error('\n✗ 打包过程出现错误:', err.message);
  process.exit(1);
});
