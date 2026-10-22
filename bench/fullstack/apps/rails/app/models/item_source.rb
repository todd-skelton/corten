require "net/http"

# ITEM_SOURCE=fixture: API responses from FIXTURE_PATH, converted once, so the benchmark measures
# the web server alone. Otherwise every call fetches the item from the item API, over one
# keep-alive connection per Puma thread.
module ItemSource
  BASE = URI(ENV.fetch("API_BASE", "http://localhost:8080"))

  def self.fixture
    return @fixture if defined?(@fixture)
    @fixture = if ENV["ITEM_SOURCE"] == "fixture"
      JSON.parse(File.read(ENV.fetch("FIXTURE_PATH", "fixtures/items.json"))).transform_values { |a| ItemView.from_api(a) }.freeze
    end
  end

  def self.get(id)
    return fixture[id] if fixture
    http = Thread.current[:item_api] ||= Net::HTTP.start(BASE.host, BASE.port)
    res = http.get("/api/discovery/items/#{ERB::Util.url_encode(id)}")
    return nil if res.code == "404"
    raise "item API returned #{res.code}" unless res.is_a?(Net::HTTPSuccess)
    ItemView.from_api(JSON.parse(res.body))
  end
end
