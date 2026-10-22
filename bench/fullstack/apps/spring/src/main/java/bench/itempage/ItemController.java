package bench.itempage;

import java.util.Comparator;
import java.util.stream.IntStream;
import jakarta.servlet.http.HttpServletResponse;
import org.springframework.http.ResponseEntity;
import org.springframework.stereotype.Controller;
import org.springframework.ui.Model;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.PostMapping;
import org.springframework.web.bind.annotation.RequestHeader;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.ResponseBody;

@Controller
public class ItemController {
    private final ItemSource items;

    public ItemController(ItemSource items) {
        this.items = items;
    }

    @GetMapping("/")
    public String home(Model model) {
        model.addAttribute("numbers", IntStream.rangeClosed(1, 50).boxed().toList());
        return "home";
    }

    @GetMapping("/items/{id}")
    public String item(@PathVariable String id, @RequestParam(required = false) String offers, Model model, HttpServletResponse res) {
        return render(items.get(id), "all".equals(offers), "", model, res);
    }

    @PostMapping("/items/{id}")
    public String alert(@PathVariable String id, @RequestParam(defaultValue = "") String title, @RequestParam(defaultValue = "") String email,
            @RequestParam(name = "target_price", defaultValue = "") String targetPrice,
            @RequestHeader(name = "HX-Request", required = false) String htmx, Model model, HttpServletResponse res) {
        var item = items.get(id);
        var message = item == null ? "" : ItemView.alertMessage(title, email, targetPrice);
        // htmx swaps only the message; without JavaScript the whole page comes back.
        if (item != null && htmx != null) {
            model.addAttribute("message", message);
            return "item :: message";
        }
        return render(item, false, message, model, res);
    }

    private String render(ItemView item, boolean allOffers, String message, Model model, HttpServletResponse res) {
        if (item == null) {
            res.setStatus(404);
            return "not-found";
        }
        model.addAttribute("item", item);
        model.addAttribute("pageTitle", item.title() + " | Card Market");
        model.addAttribute("listings", item.listings().stream().sorted(Comparator.comparingDouble(ItemView.Listing::priceValue)).toList());
        model.addAttribute("offers", allOffers ? item.offers() : item.offers().subList(0, Math.min(5, item.offers().size())));
        model.addAttribute("moreOffers", item.offers().size() > 5 && !allOffers);
        model.addAttribute("message", message);
        return "item";
    }

    /** The item as JSON for other services: the same data the page renders. */
    @GetMapping("/api/items/{id}")
    @ResponseBody
    public ResponseEntity<ItemView> json(@PathVariable String id) {
        var item = items.get(id);
        return item == null ? ResponseEntity.notFound().build() : ResponseEntity.ok(item);
    }
}
