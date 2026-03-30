import { Routes, Route } from "react-router-dom";
import { Layout } from "./Layout";
import { ReplayPage } from "./pages/ReplayPage";
import { AboutPage } from "./pages/About";
import { NewGamePage } from "./pages/NewGamePage";
import { AuthPage } from "./pages/AuthPage";

export default function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<ReplayPage />} />
        <Route path="about" element={<AboutPage />} />
        <Route path="game/new" element={<NewGamePage />} />
        <Route path="auth" element={<AuthPage />} />
      </Route>
    </Routes>
  );
}
