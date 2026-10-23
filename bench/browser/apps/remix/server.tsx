import { renderToString } from "remix/component/server";
import { RowsBench } from "./RowsBench.tsx";

export const render = async (initial: { id: number; label: string }[]) => ({ head: "", html: await renderToString(<RowsBench initial={initial} />) });
