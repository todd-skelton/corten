# Load the fixture (ITEM_SOURCE=fixture) before the first request.
Rails.application.config.after_initialize { ItemSource.fixture }
