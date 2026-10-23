import { createRoot } from "remix/component";
import { RowsBench } from "./RowsBench.tsx";

const root = createRoot(document.getElementById("main")!);
root.render(<RowsBench />);
root.flush();
document.documentElement.dataset.ready = String(performance.now());
