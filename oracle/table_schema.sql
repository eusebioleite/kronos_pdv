--drop table cards;

create table cards (
   tipo       varchar2(20) not null,
   pedido     varchar2(40) not null,
   indice     number default 0 not null,
   status     varchar2(20) default 'NOVO' not null,
   retries    number default 0 not null,
   last_error clob,
   code       number(12,0),
   guid       varchar2(50),
   created_at date default sysdate not null,
   updated_at date default sysdate not null,
   constraint pk_cards primary key ( tipo,
                                     pedido,
                                     indice )
);

create or replace trigger trg_cards_updated_at before
   update on cards
   for each row
begin
   :new.updated_at := sysdate;
end;

comment on table cards is
   'Fila de integração de pedidos de venda no DealerCRM.';

comment on column cards.tipo is
   'Tipo do pedido: ABERTO ou FECHADO.';

comment on column cards.pedido is
   'Código do pedido no ERP.';

comment on column cards.indice is
   'Índice da programação de venda no ERP. Para pedidos fechados o valor é 0.';

comment on column cards.status is
   'Estado da sincronização: ATUALIZAR, NOVO, EXCLUIR, TRAVADO, SUCESSO, EXCLUIDO.';

comment on column cards.retries is
   'Contador de tentativas de sincronizar, máximo de 5.';

comment on column cards.last_error is
   'Ultimo erro de sincronização.';

comment on column cards.code is
   'ActivityCode retornado pela API DealerCRM após a criação do card, usado para PATCH e DELETE.';

comment on column cards.guid is
   'ActivityGuid retornado pela API DealerCRM, usado para PATCH e DELETE.';

comment on column cards.created_at is
   'Timestamp de criação.';

comment on column cards.updated_at is
   'Timestamp da última modificação.';

create index idx_cards_status on
   cards (
      status
   );

grant all privileges on inventario.cards to kronos;