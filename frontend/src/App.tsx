import { BrowserRouter } from "react-router-dom";
import { ThemeProvider } from "./theme";
import AppRoot from "./surfaces/AppRoot";

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <AppRoot />
      </BrowserRouter>
    </ThemeProvider>
  );
}
