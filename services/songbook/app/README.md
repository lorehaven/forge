# songbook-app

The screens: Library, Pipeline, Discover and Activity, with the side panel of followed artists. The app does no I/O: it asks its host for data (`Request`) and is told what came (`Msg`), so the same screens run in the browser and in tests. The translations are `service/i18n/*.ftl`.
