import { useState, useEffect } from 'react';
import type { Config } from '../types';

interface Props {
  config: Config;
  onAuthorized: (token: string) => void;
  onLogout: () => void;
}

interface DeviceCodeData {
  user_code: string;
  verification_uri: string;
  verification_uri_complete: string;
  device_code: string;
  interval: number;
}

type AuthState = 'idle' | 'code_shown' | 'authorized';

export function AuthSection({ config, onAuthorized, onLogout }: Props) {
  const [authState, setAuthState] = useState<AuthState>(config.apiToken ? 'authorized' : 'idle');
  const [codeData, setCodeData] = useState<DeviceCodeData | null>(null);
  const [error, setError] = useState('');

  // Sync auth state when config.apiToken changes (e.g. background sets it)
  useEffect(() => {
    if (config.apiToken) setAuthState('authorized');
  }, [config.apiToken]);

  // Listen for AUTH_SUCCESS / AUTH_ERROR messages from background
  useEffect(() => {
    const handler = (msg: { type: string; token?: string; error?: string }) => {
      if (msg.type === 'AUTH_SUCCESS' && msg.token) {
        onAuthorized(msg.token);
        setAuthState('authorized');
      } else if (msg.type === 'AUTH_ERROR') {
        setError(msg.error ?? 'Authorization failed');
        setAuthState('idle');
        setCodeData(null);
      }
    };
    chrome.runtime.onMessage.addListener(handler);
    return () => chrome.runtime.onMessage.removeListener(handler);
  }, [onAuthorized]);

  // Resume polling UI if background is already polling
  useEffect(() => {
    chrome.runtime.sendMessage({ type: 'GET_POLLING_STATUS' }, (status: {
      isPolling: boolean;
      userCode: string | null;
      verificationUri: string | null;
      verificationUriComplete: string | null;
    }) => {
      if (chrome.runtime.lastError) return;
      if (status?.isPolling && status.userCode) {
        setCodeData({
          user_code: status.userCode,
          verification_uri: status.verificationUri ?? '',
          verification_uri_complete: status.verificationUriComplete ?? '',
          device_code: '',
          interval: 5,
        });
        setAuthState('code_shown');
      }
    });
  }, []);

  const startAuth = async () => {
    setError('');
    try {
      const res = await fetch(`${config.apiBaseUrl}/auth/device/code`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ client_name: 'Chrome Extension' }),
      });
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);

      const data = (await res.json()) as DeviceCodeData;

      let completeUrl = data.verification_uri_complete;
      if (completeUrl.startsWith('/')) completeUrl = config.apiBaseUrl + completeUrl;

      setCodeData({ ...data, verification_uri_complete: completeUrl });
      setAuthState('code_shown');

      chrome.runtime.sendMessage({
        type: 'START_POLLING',
        deviceCode: data.device_code,
        userCode: data.user_code,
        verificationUri: data.verification_uri,
        verificationUriComplete: completeUrl,
        interval: data.interval,
        apiBaseUrl: config.apiBaseUrl,
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Failed to start authorization');
    }
  };

  const openVerificationUrl = (url: string) => {
    chrome.tabs.create({ url });
  };

  if (authState === 'authorized') {
    return (
      <div className="auth-section">
        <div className="auth-heading">Authorization</div>
        <div className="auth-ok">
          <p className="auth-ok-msg">✓ Device Authorized</p>
          <button className="btn-secondary" onClick={onLogout}>
            Logout
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-section">
      <div className="auth-heading">Device Login</div>

      {authState === 'idle' && (
        <>
          <p className="auth-instruction">
            Click Start, then enter the code on the authorization page to connect this extension.
          </p>
          {error && <p className="error-msg">{error}</p>}
          <button className="btn-primary" onClick={startAuth}>
            Start Authorization
          </button>
        </>
      )}

      {authState === 'code_shown' && codeData && (
        <>
          <div className="code-box">
            <div className="user-code">{codeData.user_code}</div>
            <div className="code-sub">
              <span className="code-sub-label">Enter this code at</span>
              <a
                className="verification-link"
                href={codeData.verification_uri_complete}
                onClick={(e) => { e.preventDefault(); openVerificationUrl(codeData.verification_uri_complete); }}
              >
                {codeData.verification_uri}
              </a>
            </div>
          </div>
          <div className="polling-row">
            <div className="spinner" />
            <span>Waiting for approval...</span>
          </div>
        </>
      )}
    </div>
  );
}
