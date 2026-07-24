import { useEffect, useRef, useState } from 'react';
import { Alert } from 'antd';
import { invoke } from '@tauri-apps/api/core';
import Toolbar from '../components/Toolbar';
import './Settings.less';

function Settings() {
  const [serverUrl, setServerUrl] = useState('');
  const [saved, setSaved] = useState(false);
  const [fingerprint, setFingerprint] = useState<string | null>(null);
  const [privKeyPath, setPrivKeyPath] = useState('');
  const [pubKeyPath, setPubKeyPath] = useState('');
  const [loadError, setLoadError] = useState('');
  const [loadingKey, setLoadingKey] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    setServerUrl(localStorage.getItem('server_url') ?? '');
    setPrivKeyPath(localStorage.getItem('priv_key_path') ?? '');
    setPubKeyPath(localStorage.getItem('pub_key_path') ?? '');
    loadFingerprint();
  }, []);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const loadFingerprint = async () => {
    try {
      const fp = await invoke<string | null>('get_keypair_fingerprint');
      setFingerprint(fp);
    } catch {
      setFingerprint(null);
    }
  };

  const handleSave = () => {
    localStorage.setItem('server_url', serverUrl);
    setSaved(true);
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setSaved(false), 2000);
  };

  const handleLoadKey = async () => {
    if (!privKeyPath || !pubKeyPath) {
      setLoadError('请填写私钥和公钥文件路径');
      return;
    }
    setLoadingKey(true);
    setLoadError('');
    try {
      const fp = await invoke<string>('load_keypair', {
        privKeyPath,
        pubKeyPath,
      });
      setFingerprint(fp);
      localStorage.setItem('priv_key_path', privKeyPath);
      localStorage.setItem('pub_key_path', pubKeyPath);
    } catch (e: any) {
      setLoadError(e?.toString() ?? '加载失败');
    } finally {
      setLoadingKey(false);
    }
  };

  return (
    <div className="settings-page">
      <Toolbar title="设置" />
      <div className="ant-card settings-form">
        <div className="settings-field">
          <label className="settings-label">后端服务地址</label>
          <input
            className="ant-input settings-input"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
            placeholder="http://localhost:8080"
          />
        </div>
        <div className="settings-actions">
          <button className="ant-btn ant-btn-primary" onClick={handleSave}>
            保存
          </button>
          {saved && <span className="ant-tag ant-tag-success">已保存</span>}
        </div>

        <div className="settings-divider" />

        <div className="settings-field">
          <label className="settings-label">SSH 密钥配置</label>

          <div className="settings-field">
            <label className="settings-sublabel">私钥文件路径</label>
            <input
              className="ant-input settings-input"
              value={privKeyPath}
              onChange={(e) => setPrivKeyPath(e.target.value)}
              placeholder="C:\Users\...\.ssh\id_ed25519"
            />
          </div>

          <div className="settings-field">
            <label className="settings-sublabel">公钥文件路径</label>
            <input
              className="ant-input settings-input"
              value={pubKeyPath}
              onChange={(e) => setPubKeyPath(e.target.value)}
              placeholder="C:\Users\...\.ssh\id_ed25519.pub"
            />
          </div>

          <label className="settings-sublabel">SSH 公钥 Fingerprint</label>
          <div className="settings-fingerprint">
            {fingerprint || '未加载'}
          </div>

          <div className="settings-actions">
            <button
              className="ant-btn ant-btn-primary"
              onClick={handleLoadKey}
              disabled={loadingKey}
            >
              {loadingKey ? '保存中...' : '保存密钥'}
            </button>
          </div>

          {loadError && <Alert type="error" message={loadError} closable showIcon />}

          <div className="settings-hint">
            使用 ssh-keygen -t ed25519 生成密钥对，将公钥添加到服务端 admin_key 表。
          </div>
        </div>
      </div>
    </div>
  );
}

export default Settings;
