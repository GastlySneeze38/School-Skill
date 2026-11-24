import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { LazyMotion, MotionConfig, domAnimation } from "motion/react";
import App from "./App";
import "./index.css";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    {/* LazyMotion : seules les fonctions d'animation utilisées sont chargées (composants `m.*`).
        `strict` refuse les `motion.*` complets par erreur. MotionConfig : si Windows demande
        moins d'animations, les déplacements sont coupés et seuls les fondus restent. */}
    <LazyMotion features={domAnimation} strict>
      <MotionConfig reducedMotion="user">
        <App />
      </MotionConfig>
    </LazyMotion>
  </StrictMode>,
);
