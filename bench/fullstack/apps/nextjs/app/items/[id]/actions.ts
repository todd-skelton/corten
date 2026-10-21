"use server";
import { alertMessage } from "@/lib/view-model";

export async function createAlert(_previous: { message: string }, form: FormData): Promise<{ message: string }> {
  return { message: alertMessage(String(form.get("title") ?? ""), String(form.get("email") ?? ""), String(form.get("target_price") ?? "")) };
}
