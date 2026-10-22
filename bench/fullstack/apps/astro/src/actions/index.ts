import { defineAction } from "astro:actions";
import { z } from "astro/zod";
import { alertMessage } from "../lib/view-model";

export const server = {
  alert: defineAction({
    accept: "form",
    input: z.object({ title: z.string(), email: z.string(), target_price: z.string() }),
    handler: ({ title, email, target_price }) => ({ message: alertMessage(title, email, target_price) }),
  }),
};
