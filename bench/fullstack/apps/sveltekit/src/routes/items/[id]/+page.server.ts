import { error } from "@sveltejs/kit";
import { fetchItem } from "#lib/server/api";
import { alertMessage } from "#lib/view-model";
import type { Actions, PageServerLoad } from "./$types";

export const load: PageServerLoad = async ({ params }) => {
  const item = await fetchItem(params.id);
  if (!item) error(404, "Not found");
  return { item };
};

export const actions: Actions = {
  default: async ({ request }) => {
    const form = await request.formData();
    return { message: alertMessage(String(form.get("title") ?? ""), String(form.get("email") ?? ""), String(form.get("target_price") ?? "")) };
  },
};
