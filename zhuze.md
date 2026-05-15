1. 安装rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh


2. 测试安装rust版本
rustc --version
cargo --version

3. 环境变量配置
source "$HOME/.cargo/env"

4.1. 执行安装命令(已建立软链接)
bash install.sh

4.2. 手动建立软链接
bash link-claw.sh

5. 手动构建当前工作空间中包含的所有成员包
cd rust
cargo build --workspace

6. 配置大模型环境变量
~/.claw/config.json 配置参数

7. 启动clawcode
任务位置执行 claw