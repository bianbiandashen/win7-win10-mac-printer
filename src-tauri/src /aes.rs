use aes::Aes128;
use aes::cipher::{BlockDecrypt, KeyInit};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use log::{info, error};
use tauri::command;

fn pkcs7_unpad(data: &[u8]) -> Option<Vec<u8>> {
    if data.is_empty() {
        return None;
    }
    
    let padding_len = *data.last()? as usize;
    if padding_len == 0 || padding_len > 16 {
        return None;
    }
    
    let message_len = data.len().checked_sub(padding_len)?;
    let padding = &data[message_len..];
    
    if padding.iter().all(|&x| x == padding_len as u8) {
        Some(data[..message_len].to_vec())
    } else {
        None
    }
}

#[command]
pub fn decrypt_aes_encrypted_string(encrypted_str: &str) -> Result<String, String> {
    // 记录入参
    // info!("【解密函数】收到加密字符串: {}", encrypted_str);

    // Java Signed Byte	Calculation	Rust Unsigned Byte	Hexadecimal
    // -52	256 + (-52) = 204	204	0xCC
    // -77	256 + (-77) = 179	179	0xB3
    // -54	256 + (-54) = 202	202	0xCA
    // -109	256 + (-109) = 147	147	0x93
    // 29	29	29	0x1D
    // -52	256 + (-52) = 204	204	0xCC
    // 113	113	113	0x71
    // -118	256 + (-118) = 138	138	0x8A
    // -22	256 + (-22) = 234	234	0xEA
    // -57	256 + (-57) = 199	199	0xC7
    // -47	256 + (-47) = 209	209	0xD1
    // 113	113	113	0x71
    // -13	256 + (-13) = 243	243	0xF3
    // -83	256 + (-83) = 173	173	0xAD
    // -100	256 + (-100) = 156	156	0x9C
    // 90	90	90	0x5A
    // 转换后的 Rust 密钥字节数组
    // rust

    // let key_bytes: [u8; 16] = [
    //     204, 179, 202, 147, 29, 204, 113, 138,
    //     234, 199, 209, 113, 243, 173, 156, 90,
    // ];

    // AES密钥字节数组（已修正）
    let key_bytes: [u8; 16] = [
        204, 179, 202, 147, 29, 204, 113, 138,
        234, 199, 209, 113, 243, 173, 156, 90,
    ];
    // info!("【解密函数】使用的 AES 密钥字节数组: {:?}", key_bytes);

    // 去掉前缀 "AES:" 并记录
    let ciphertext_base64 = encrypted_str.trim_start_matches("AES:");
    // info!("【解密函数】去除前缀后的 Base64 密文: {}", ciphertext_base64);

    let ciphertext = BASE64.decode(ciphertext_base64).map_err(|e| {
        error!("【解密函数】Base64 解码失败: {:?}", e);
        format!("Base64 解码失败: {:?}", e)
    })?;

    // info!("【解密函数】Base64 解码成功，得到的密文字节数组长度: {}", ciphertext.len());
    // info!("【解密函数】解码后的密文字节数组: {:?}", ciphertext);

    // 创建 AES-128-ECB 解密器
    let cipher = Aes128::new_from_slice(&key_bytes).map_err(|e| {
        error!("【解密函数】创建解密器失败: {:?}", e);
        format!("创建解密器失败: {:?}", e)
    })?;

    // 按块解密
    let mut decrypted = Vec::new();
    for chunk in ciphertext.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        
        let mut block = aes::Block::from(block);
        cipher.decrypt_block(&mut block);
        
        decrypted.extend_from_slice(&block);
    }

    // 去除 PKCS7 填充
    let unpadded = pkcs7_unpad(&decrypted).ok_or_else(|| {
        error!("【解密函数】PKCS7 去填充失败");
        "PKCS7 去填充失败".to_string()
    })?;

    // info!("【解密函数】解密成功，去除填充后的数据长度: {}", unpadded.len());
    // info!("【解密函数】解密后的数据（字节数组）: {:?}", unpadded);

    // 转换为 UTF-8 字符串
    let result = String::from_utf8(unpadded).map_err(|e| {
        error!("【解密函数】解密结果转换为字符串失败: {:?}", e);
        format!("解密结果转换失败: {:?}", e)
    })?;

    info!("【解密函数】解密后的字符串: cpCode 等主模版中的解释数据{}", result);
    Ok(result)
}


