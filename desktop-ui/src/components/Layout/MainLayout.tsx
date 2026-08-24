import { Outlet } from 'react-router-dom';
import { Toaster } from 'sonner';
import { Sidebar } from './Sidebar';

export const MainLayout = () => {
  return (
    <div className="layout">
      <Toaster theme="dark" position="top-right" />
      <Sidebar />
      <main className="main-content">
        <div className="container">
          <Outlet />
        </div>
      </main>
    </div>
  );
};
