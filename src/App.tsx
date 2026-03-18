/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

import "./App.css";
import { useState } from "react";
import { LogInScreen } from "./screens/Log-in";
import { RegistrationScreen } from "./screens/registration";

type EntryScreen = "login" | "registration";

type LoginLaunchState = {
  homeserver?: string;
  text: string;
  tone: "error" | "success" | "info";
};

function App() {
  const [activeScreen, setActiveScreen] = useState<EntryScreen>("login");
  const [loginLaunchState, setLoginLaunchState] = useState<LoginLaunchState | null>(null);

  if (activeScreen === "registration") {
    return (
      <RegistrationScreen
        onBackToLogin={(nextLaunchState) => {
          setLoginLaunchState(nextLaunchState ?? null);
          setActiveScreen("login");
        }}
      />
    );
  }

  return (
    <LogInScreen
      initialFeedback={loginLaunchState}
      initialHomeserver={loginLaunchState?.homeserver}
      onOpenRegistration={() => {
        setLoginLaunchState(null);
        setActiveScreen("registration");
      }}
    />
  );
}

export default App;
