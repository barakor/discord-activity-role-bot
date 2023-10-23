FROM alpine:3

RUN apk add clojure bash
RUN wget https://raw.githubusercontent.com/technomancy/leiningen/stable/bin/lein -O /usr/local/bin/lein
RUN chmod a+x /usr/local/bin/lein

COPY ./ /app
WORKDIR /app

RUN lein deps

EXPOSE 443
EXPOSE 80

CMD lein run
