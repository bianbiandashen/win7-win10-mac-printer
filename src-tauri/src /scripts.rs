pub const INSTALL_CERT_SCRIPT: &str = r#"set shCmds to "function appendIfNotExist(){
  local target=$1
  echo $target
  local content=$2
    echo $content
  local check=`cat $target | grep \"$content\"`
  if [ ${check}x == ''x ]
  then
      echo $content >> $target
  fi
}
security add-trusted-cert -d -r trustAsRoot -k /Library/Keychains/System.keychain ./localhost.cer
hosts=/etc/hosts
name=`hostname`
appendIfNotExist $hosts '127.0.0.1 localhost'
appendIfNotExist $hosts '127.0.0.1 printer.xiaohongshu.com'
appendIfNotExist $hosts '::1 localhost'
appendIfNotExist $hosts \"::1 $name\"
appendIfNotExist $hosts \"127.0.0.1 $name\""
set titleStr to "您授权后,我们将执行两个操作
1. 为您安装 localhost 域名证书,以便浏览器安全的使用 https 访问 localhost
1. 为您安装 printer.xiaohongshu.com 域名证书,以便浏览器安全的使用 https 访问 printer.xiaohongshu.com
2. 修改 /etc/hosts,以添加 localhost 到 127.0.0.1 和 ::1 的映射,以规避 mac 系统的错误寻址导致的应用启动慢问题
3. 如果安装后
如果您不知道这些是什么,请选择确认"
set btns to {"确认", "取消"}
display dialog titleStr buttons btns default button 1 with icon caution
get the button returned of the result
if result = "确认" then
 try
 do shell script shCmds with administrator privileges
 return 0
 on error errMsg number errNo
 display alert "授权失败 " & ¬
 errNo & " " & errMsg
 return errNo
 end try
else
 return -1
end if"#; 