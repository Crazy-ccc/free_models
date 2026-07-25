import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Sidebar from './components/Sidebar';
import Overview from './pages/Overview';
import Providers from './pages/Providers';
import Models from './pages/Models';
import ApiKeys from './pages/ApiKeys';
import Settings from './pages/Settings';
import Stats from './pages/Stats';
import type { PageKey } from './types';

function App() {
  const [activePage, setActivePage] = useState<PageKey>('overview');
  const [serviceConnected, setServiceConnected] = useState(false);

  useEffect(() => {
    const pvt = localStorage.getItem('priv_key_path');
    const pub = localStorage.getItem('pub_key_path');
    if (pvt && pub) {
      invoke('load_keypair', { privKeyPath: pvt, pubKeyPath: pub })
        .then(() => {
          const url = localStorage.getItem('server_url');
          if (url) {
            invoke('get_service_status', { serverUrl: url })
              .then(() => setServiceConnected(true))
              .catch(() => setServiceConnected(false));
          }
        })
        .catch(() => {});
    }
  }, []);

  const renderPage = () => {
    switch (activePage) {
      case 'overview':
        return <Overview onStatusChange={setServiceConnected} />;
      case 'providers':
        return <Providers />;
      case 'models':
        return <Models />;
      case 'apiKeys':
        return <ApiKeys />;
      case 'stats':
        return <Stats />;
      case 'settings':
        return <Settings />;
      default:
        return null;
    }
  };

  return (
    <div className="app-shell">
      <Sidebar
        activePage={activePage}
        onNavigate={setActivePage}
        serviceConnected={serviceConnected}
      />
      <main className="app-main">{renderPage()}</main>
    </div>
  );
}

export default App;
