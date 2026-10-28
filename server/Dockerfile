FROM php:8.2-cli
RUN docker-php-ext-install mysqli opcache >/dev/null \
 && printf "memory_limit=1G\nzend.exception_ignore_args=On\nopcache.enable_cli=1\nopcache.validate_timestamps=1\nopcache.revalidate_freq=0\n" > /usr/local/etc/php/conf.d/arena.ini
WORKDIR /server
ENV PHP_CLI_SERVER_WORKERS=6
CMD ["php", "-S", "0.0.0.0:8080", "public/index.php"]
