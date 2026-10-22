import type { ActionFunctionArgs } from "react-router";
import { publish } from "../live.server";

export async function action({ request }: ActionFunctionArgs) {
  return publish(await request.text());
}
