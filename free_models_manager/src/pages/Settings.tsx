import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Toolbar from '../components/Toolbar';
import { DoodleButton, DoodleInput } from '../components/doodle';
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
        <div className="settings-section-title">服务地址</div>

        <div className="settings-field">
          <label className="settings-label">后端服务地址</label>
          <DoodleInput
            className="settings-input"
            value={serverUrl}
            onChange={(e) => setServerUrl(e.target.value)}
            placeholder="http://localhost:8080"
          />
        </div>
        <div className="settings-actions">
          <DoodleButton type="primary" onClick={handleSave}>
            保存
          </DoodleButton>
          {saved && <span className="ant-tag ant-tag-success">已保存</span>}
        </div>

        <div className="settings-divider" />

        <div className="settings-section-title">SSH 密钥配置</div>

        <div className="settings-field">
          <label className="settings-label">私钥文件路径</label>
          <DoodleInput
            className="settings-input"
            value={privKeyPath}
            onChange={(e) => setPrivKeyPath(e.target.value)}
            placeholder="C:\Users\...\.ssh\id_ed25519"
          />
        </div>

        <div className="settings-field">
          <label className="settings-label">公钥文件路径</label>
          <DoodleInput
            className="settings-input"
            value={pubKeyPath}
            onChange={(e) => setPubKeyPath(e.target.value)}
            placeholder="C:\Users\...\.ssh\id_ed25519.pub"
          />
        </div>

        <div className="settings-field">
          <label className="settings-label">SSH 公钥 Fingerprint</label>
          <div className="settings-fingerprint">
            {fingerprint || '未加载'}
          </div>
        </div>

        <div className="settings-actions">
          <DoodleButton
            type="teal"
            onClick={handleLoadKey}
            disabled={loadingKey}
          >
            {loadingKey ? '保存中...' : '保存密钥'}
          </DoodleButton>
        </div>

        {loadError && (
          <div className="settings-error-card">
            <span className="settings-error-text">{loadError}</span>
            <DoodleButton
              type="ghost"
              size="small"
              className="settings-error-close"
              onClick={() => setLoadError('')}
            >
              ✕
            </DoodleButton>
          </div>
        )}

        <div className="settings-hint">
          使用 ssh-keygen -t ed25519 生成密钥对，将公钥添加到服务端 admin_key 表。
        </div>
      </div>
    </div>
  );
}

export default Settings;