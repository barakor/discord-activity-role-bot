FROM clojure:temurin-17-lein-alpine

COPY ./ /app
WORKDIR /app

RUN lein deps

EXPOSE 443
EXPOSE 80

CMD lein run
