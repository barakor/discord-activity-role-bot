FROM clojure:temurin-21-lein

COPY ./ /app
WORKDIR /app


EXPOSE 443
EXPOSE 80

CMD lein run
