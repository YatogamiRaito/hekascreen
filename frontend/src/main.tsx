import { createRoot } from "react-dom/client";
import "virtual:uno.css";
import "./index.css";
import router from "./router";
import { Provider } from "react-redux";
import { store, useAppSelector } from "./store/store.ts";
import { ConfigProvider, theme } from "antd";
import "./i18n";
import { RouterProvider } from "react-router-dom";

function AppWrapper() {
  const localConfigTheme = useAppSelector((state) => state.localConfig.theme);

  return (
    <ConfigProvider
      theme={{
        algorithm: localConfigTheme === "light" ? theme.defaultAlgorithm : theme.darkAlgorithm,
        cssVar: { prefix: "ant" },
        hashed: false,
        token: {
          colorPrimary: "#b72a20",
          colorInfo: "#1677ff",
          colorLink: "#1677ff",
          colorSuccess: "#52c41a",
          colorError: "#de7c7d",
          colorWarning: "#c1840c",
          colorBgBase: localConfigTheme === "light" ? "#ffffff" : "#060606",
        },
      }}
    >
      <RouterProvider router={router} />
    </ConfigProvider>
  );
}

createRoot(document.getElementById("root")!).render(
  <Provider store={store}>
    <AppWrapper />
  </Provider>
);
