use std::process::{Command, Stdio};
use std::io::{self, Write, BufRead, BufReader};
use std::fs::{self, File, OpenOptions};
use std::path::Path;
use std::env;

fn main() {
    println!("🚀 开始服务器环境安装...");
    
    // 获取用户输入
    let docker_username = get_input("请输入Docker镜像账户: ");
    let docker_password = get_input("请输入Docker镜像密码: ");
    let samba_password = get_input("请输入Samba密码: ");
    let cpolar_token = get_input("请输入cpolar认证token: ");
    let su_password = get_input("请输入su密码: ");
    let clash_subscription = get_input("请输入Clash订阅链接 (可选): ");
    
    // 获取系统架构
    let arch = get_system_arch();
    println!("检测到系统架构: {}", arch);
    
    let steps: Vec<(&str, Box<dyn Fn() -> Result<(), Box<dyn std::error::Error>>>)> = vec![
        ("设置su密码", Box::new({
            let password = su_password.clone();
            move || setup_su_password(&password)
        })),
        ("安装Rust", Box::new(|| install_rust())),
        ("安装Samba", Box::new({
            let password = samba_password.clone();
            move || install_samba(&password)
        })),
        ("安装Docker", Box::new({
            let username = docker_username.clone();
            let password = docker_password.clone();
            move || install_docker(&username, &password)
        })),
        ("安装Caddy", Box::new(|| install_caddy())),
        ("安装cpolar", Box::new({
            let token = cpolar_token.clone();
            move || install_cpolar(&token)
        })),
        ("设置Let's Encrypt证书", Box::new(|| setup_letsencrypt())),
        ("创建app目录结构", Box::new(|| setup_app_directory())),
        ("安装版本管理工具", Box::new(|| install_version_managers())),
        ("配置SSH", Box::new(|| configure_ssh())),
        ("安装FlClash", Box::new({
            let arch = arch.clone();
            let subscription = clash_subscription.clone();
            move || install_flclash(&arch, &subscription)
        })),
    ];
    
    for (name, func) in steps {
        println!("\n📦 {}", name);
        match func() {
            Ok(_) => println!("✅ {} 完成", name),
            Err(e) => {
                eprintln!("❌ {} 失败: {}", name, e);
                std::process::exit(1);
            }
        }
    }
    
    println!("\n🎉 所有安装步骤完成!");
    println!("请重新登录以使环境变量生效，或运行: source ~/.bashrc");
}

fn get_input(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().flush().unwrap();
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn get_system_arch() -> String {
    let output = Command::new("uname")
        .arg("-m")
        .output()
        .expect("Failed to get architecture");
    
    let arch_output = String::from_utf8_lossy(&output.stdout);
    let arch = arch_output.trim();
    match arch {
        "x86_64" => "amd64".to_string(),
        "aarch64" => "arm64".to_string(),
        "armv7l" => "armhf".to_string(),
        _ => arch.to_string(),
    }
}

fn run_command(cmd: &str) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("bash")
        .arg("-c")
        .arg(cmd)
        .output()?;
    
    if !output.status.success() {
        return Err(format!("Command failed: {}\nStderr: {}", 
                          cmd, String::from_utf8_lossy(&output.stderr)).into());
    }
    Ok(())
}

fn setup_su_password(password: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cmd = format!("echo 'root:{}' | sudo chpasswd", password);
    run_command(&cmd)?;
    Ok(())
}

fn install_rust() -> Result<(), Box<dyn std::error::Error>> {
    // 更新包管理器
    run_command("sudo apt update")?;
    
    // 安装必要的依赖
    run_command("sudo apt install -y curl build-essential")?;
    
    // 设置Rust环境变量使用清华源
    run_command("export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup")?;
    run_command("export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup")?;
    
    // 安装Rust
    let install_cmd = r#"
        export RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
        export RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    "#;
    run_command(install_cmd)?;
    
    // 配置cargo使用清华源
    let home = env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".to_string());
    let cargo_config_dir = format!("{}/.cargo", home);
    fs::create_dir_all(&cargo_config_dir)?;
    
    let config_content = r#"[source.crates-io]
replace-with = 'tuna'

[source.tuna]
registry = "https://mirrors.tuna.tsinghua.edu.cn/git/crates.io-index.git"
"#;
    
    fs::write(format!("{}/config", cargo_config_dir), config_content)?;
    
    // 添加到bashrc
    let bashrc_path = format!("{}/.bashrc", home);
    let cargo_env = "\n# Rust\nsource ~/.cargo/env\n";
    append_to_file(&bashrc_path, cargo_env)?;
    
    Ok(())
}

fn install_samba(password: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 安装samba
    run_command("sudo apt install -y samba")?;
    
    // 创建共享目录
    let home = env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".to_string());
    let share_dir = format!("{}/smb-share", home);
    fs::create_dir_all(&share_dir)?;
    run_command(&format!("chmod 755 {}", share_dir))?;
    
    // 配置samba
    let samba_config = format!(r#"
[{}-share]
    path = {}
    browseable = yes
    read only = no
    guest ok = no
    valid users = {}
    create mask = 0755
    directory mask = 0755
"#, env::var("USER").unwrap_or_else(|_| "ubuntu".to_string()), 
    share_dir, 
    env::var("USER").unwrap_or_else(|_| "ubuntu".to_string()));
    
    append_to_file("/etc/samba/smb.conf", &samba_config)?;
    
    // 设置samba密码
    let user = env::var("USER").unwrap_or_else(|_| "ubuntu".to_string());
    let cmd = format!("echo -e '{}\\n{}' | sudo smbpasswd -a {}", password, password, user);
    run_command(&cmd)?;
    
    // 启动并启用samba服务
    run_command("sudo systemctl enable smbd")?;
    run_command("sudo systemctl start smbd")?;
    
    Ok(())
}

fn install_docker(username: &str, password: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 安装必要的包
    run_command("sudo apt install -y apt-transport-https ca-certificates curl gnupg lsb-release")?;
    
    // 添加Docker的官方GPG密钥（使用阿里云镜像）
    run_command("curl -fsSL https://mirrors.aliyun.com/docker-ce/linux/ubuntu/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg")?;
    
    // 添加Docker仓库
    let cmd = r#"echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://mirrors.aliyun.com/docker-ce/linux/ubuntu $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null"#;
    run_command(cmd)?;
    
    // 更新包索引并安装Docker
    run_command("sudo apt update")?;
    run_command("sudo apt install -y docker-ce docker-ce-cli containerd.io")?;
    
    // 安装docker-compose
    run_command("sudo apt install -y docker-compose")?;
    
    // 将当前用户添加到docker组
    let user = env::var("USER").unwrap_or_else(|_| "ubuntu".to_string());
    run_command(&format!("sudo usermod -aG docker {}", user))?;
    
    // 配置Docker镜像源
    let daemon_json = r#"{
    "registry-mirrors": [
        "https://docker.xuanyuan.dev",
        "https://dockerhub.azk8s.cn",
        "https://mirror.ccs.tencentyun.com"
    ],
    "insecure-registries": ["docker.xuanyuan.dev"]
}"#;
    
    fs::create_dir_all("/etc/docker")?;
    fs::write("/etc/docker/daemon.json", daemon_json)?;
    
    // 重启Docker服务
    run_command("sudo systemctl daemon-reload")?;
    run_command("sudo systemctl restart docker")?;
    run_command("sudo systemctl enable docker")?;
    
    // 登录到私有镜像仓库
    let login_cmd = format!("echo '{}' | docker login -u {} --password-stdin docker.xuanyuan.dev", password, username);
    run_command(&login_cmd)?;
    
    Ok(())
}

fn install_caddy() -> Result<(), Box<dyn std::error::Error>> {
    // 安装依赖
    run_command("sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https")?;
    
    // 添加Caddy仓库
    run_command("curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg")?;
    run_command("curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list")?;
    
    // 安装Caddy
    run_command("sudo apt update")?;
    run_command("sudo apt install -y caddy")?;
    
    // 启用服务
    run_command("sudo systemctl enable caddy")?;
    
    Ok(())
}

fn install_cpolar(token: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 下载并安装cpolar
    run_command("curl -L https://www.cpolar.com/static/downloads/install-release-cpolar.sh | sudo bash")?;
    
    // 认证登录
    let auth_cmd = format!("cpolar authtoken {}", token);
    run_command(&auth_cmd)?;
    
    // 启用服务
    run_command("sudo systemctl enable cpolar")?;
    run_command("sudo systemctl start cpolar")?;
    
    Ok(())
}

fn setup_letsencrypt() -> Result<(), Box<dyn std::error::Error>> {
    // 安装certbot
    run_command("sudo apt install -y certbot")?;
    
    // 创建证书更新脚本
    let renewal_script = r#"#!/bin/bash
certbot renew --quiet
systemctl reload caddy
"#;
    
    fs::write("/usr/local/bin/cert-renewal.sh", renewal_script)?;
    run_command("sudo chmod +x /usr/local/bin/cert-renewal.sh")?;
    
    // 添加到crontab (每月1号凌晨2点执行)
    run_command("(crontab -l 2>/dev/null; echo '0 2 1 * * /usr/local/bin/cert-renewal.sh') | crontab -")?;
    
    Ok(())
}

fn setup_app_directory() -> Result<(), Box<dyn std::error::Error>> {
    let home = env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".to_string());
    let app_dir = format!("{}/app", home);
    
    // 创建app目录
    fs::create_dir_all(&app_dir)?;
    
    // 复制到/usr/local/
    run_command(&format!("sudo cp -r {} /usr/local/", app_dir))?;
    
    // 创建递归PATH添加脚本
    let path_script = format!(r#"#!/bin/bash
# 递归添加app目录下的所有二进制文件到PATH
add_to_path() {{
    local dir="$1"
    if [ -d "$dir" ]; then
        # 添加当前目录到PATH（如果包含可执行文件）
        if find "$dir" -maxdepth 1 -type f -executable | grep -q .; then
            export PATH="$dir:$PATH"
        fi
        
        # 递归处理子目录
        for subdir in "$dir"/*; do
            if [ -d "$subdir" ]; then
                add_to_path "$subdir"
            fi
        done
    fi
}}

# 添加用户app目录
add_to_path "{}/app"
# 添加系统app目录
add_to_path "/usr/local/app"
"#, home);
    
    // 写入到用户的.bashrc
    let bashrc_content = format!("\n# App directories\n{}\n", path_script);
    append_to_file(&format!("{}/.bashrc", home), &bashrc_content)?;
    
    // 写入到root的.bashrc
    let root_bashrc_content = r#"
# App directories
add_to_path() {
    local dir="$1"
    if [ -d "$dir" ]; then
        if find "$dir" -maxdepth 1 -type f -executable | grep -q .; then
            export PATH="$dir:$PATH"
        fi
        for subdir in "$dir"/*; do
            if [ -d "$subdir" ]; then
                add_to_path "$subdir"
            fi
        done
    fi
}
add_to_path "/usr/local/app"
"#;
    
    run_command(&format!("echo '{}' | sudo tee -a /root/.bashrc", root_bashrc_content))?;
    
    Ok(())
}

fn install_version_managers() -> Result<(), Box<dyn std::error::Error>> {
    let home = env::var("HOME").unwrap_or_else(|_| "/home/ubuntu".to_string());
    
    // 安装pyenv
    run_command("sudo apt install -y make build-essential libssl-dev zlib1g-dev libbz2-dev libreadline-dev libsqlite3-dev wget curl llvm libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev")?;
    
    // 使用清华源安装pyenv
    let pyenv_install = format!(r#"
        export PYENV_ROOT="{}"/.pyenv
        git clone https://mirrors.tuna.tsinghua.edu.cn/git/pyenv.git $PYENV_ROOT
    "#, home);
    run_command(&pyenv_install)?;
    
    // 配置pyenv
    let pyenv_config = format!(r#"
# pyenv
export PYENV_ROOT="{}/.pyenv"
export PATH="$PYENV_ROOT/bin:$PATH"
eval "$(pyenv init -)"
"#, home);
    append_to_file(&format!("{}/.bashrc", home), &pyenv_config)?;
    
    // 安装nvm（使用清华源）
    run_command("curl -o- https://mirrors.tuna.tsinghua.edu.cn/git/nvm.git/plain/install.sh | bash")?;
    
    // 配置npm使用淘宝源
    let npm_config = r#"
# nvm and npm
export NVM_DIR="$HOME/.nvm"
[ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
[ -s "$NVM_DIR/bash_completion" ] && \. "$NVM_DIR/bash_completion"
export NVM_NODEJS_ORG_MIRROR=https://mirrors.tuna.tsinghua.edu.cn/nodejs-release/
"#;
    append_to_file(&format!("{}/.bashrc", home), npm_config)?;
    
    // 重新加载环境
    run_command("source ~/.bashrc")?;
    
    // 安装最新版本的Python和Node.js
    let install_versions = format!(r#"
        export PYENV_ROOT="{}/.pyenv"
        export PATH="$PYENV_ROOT/bin:$PATH"
        eval "$(pyenv init -)"
        
        # 安装最新Python
        pyenv install 3.11.0
        pyenv global 3.11.0
        
        # 安装最新Node.js
        export NVM_DIR="$HOME/.nvm"
        [ -s "$NVM_DIR/nvm.sh" ] && \. "$NVM_DIR/nvm.sh"
        nvm install node
        nvm use node
        nvm alias default node
        
        # 配置npm使用淘宝源
        npm config set registry https://registry.npmmirror.com
        
        # 安装全局工具
        npm install -g bun yarn pm2
    "#, home);
    run_command(&install_versions)?;
    
    Ok(())
}

fn configure_ssh() -> Result<(), Box<dyn std::error::Error>> {
    let ssh_config = r#"
# 允许密码认证和密钥认证
PasswordAuthentication yes
PubkeyAuthentication yes
AuthorizedKeysFile .ssh/authorized_keys
PermitRootLogin yes
"#;
    
    // 备份原配置
    run_command("sudo cp /etc/ssh/sshd_config /etc/ssh/sshd_config.backup")?;
    
    // 修改SSH配置
    append_to_file("/etc/ssh/sshd_config", ssh_config)?;
    
    // 重启SSH服务
    run_command("sudo systemctl restart ssh")?;
    
    Ok(())
}

fn install_flclash(arch: &str, subscription: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 下载FlClash
    let download_url = format!("https://github.com/chen08209/FlClash/releases/download/v0.8.86/FlClash-0.8.86-linux-{}.deb", arch);
    let deb_file = "/tmp/flclash.deb";
    
    let download_cmd = format!("wget -O {} {}", deb_file, download_url);
    run_command(&download_cmd)?;
    
    // 安装deb包
    run_command(&format!("sudo dpkg -i {}", deb_file))?;
    run_command("sudo apt-get install -f")?; // 修复依赖
    
    // 创建systemd服务文件
    let service_content = r#"[Unit]
Description=FlClash
After=network.target

[Service]
Type=simple
User=nobody
ExecStart=/usr/bin/flclash -d /etc/flclash
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
"#;
    
    fs::write("/etc/systemd/system/flclash.service", service_content)?;
    
    // 创建配置目录
    run_command("sudo mkdir -p /etc/flclash")?;
    
    // 创建配置更新脚本
    let update_script = format!(r#"#!/bin/bash
SUBSCRIPTION_URL="$1"
CONFIG_DIR="/etc/flclash"
OLD_CONFIG_FILE="$CONFIG_DIR/old_subscription.txt"

if [ -z "$SUBSCRIPTION_URL" ]; then
    # 如果没有提供新链接，尝试使用旧链接
    if [ -f "$OLD_CONFIG_FILE" ]; then
        SUBSCRIPTION_URL=$(cat "$OLD_CONFIG_FILE")
        echo "使用已保存的订阅链接: $SUBSCRIPTION_URL"
    else
        echo "请提供订阅链接: $0 <subscription_url>"
        exit 1
    fi
else
    # 保存新的订阅链接
    echo "$SUBSCRIPTION_URL" > "$OLD_CONFIG_FILE"
    echo "订阅链接已保存"
fi

# 下载配置文件
echo "正在下载配置文件..."
curl -L "$SUBSCRIPTION_URL" -o "$CONFIG_DIR/config.yaml"

if [ $? -eq 0 ]; then
    echo "配置文件下载成功"
    # 重启服务
    sudo systemctl restart flclash
    echo "FlClash服务已重启"
else
    echo "配置文件下载失败"
    exit 1
fi
"#);
    
    fs::write("/usr/local/bin/update-clash-config.sh", update_script)?;
    run_command("sudo chmod +x /usr/local/bin/update-clash-config.sh")?;
    
    // 如果提供了订阅链接，初始化配置
    if !subscription.is_empty() {
        run_command(&format!("/usr/local/bin/update-clash-config.sh '{}'", subscription))?;
    }
    
    // 启用并启动服务
    run_command("sudo systemctl daemon-reload")?;
    run_command("sudo systemctl enable flclash")?;
    run_command("sudo systemctl start flclash")?;
    
    println!("FlClash安装完成!");
    println!("使用以下命令更新配置: sudo /usr/local/bin/update-clash-config.sh <订阅链接>");
    println!("查看服务状态: sudo systemctl status flclash");
    
    Ok(())
}

fn append_to_file(file_path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    if file_path.starts_with("/etc/") || file_path.starts_with("/usr/") || file_path.starts_with("/root/") {
        // 需要sudo权限的文件
        let cmd = format!("echo '{}' | sudo tee -a {}", content.replace("'", "'\"'\"'"), file_path);
        run_command(&cmd)?;
    } else {
        // 普通文件
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)?;
        file.write_all(content.as_bytes())?;
    }
    Ok(())
}