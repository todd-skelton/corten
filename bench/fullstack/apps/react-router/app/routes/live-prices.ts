import { pricesStream } from "../live.server";

export function loader() {
  return pricesStream();
}
