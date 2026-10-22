import { alertMessage } from "../../shared/view-model";

export default defineEventHandler(async (event) => {
  const form = await readFormData(event);
  return { message: alertMessage(String(form.get("title") ?? ""), String(form.get("email") ?? ""), String(form.get("target_price") ?? "")) };
});
